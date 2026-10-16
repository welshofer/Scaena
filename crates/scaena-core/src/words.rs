//! The editors' words (PLAN 3.18, 2.99): what a person reads where the deck's format has names of
//! its own. A slide is numbered, and a step is its slide's; an object is what it shows, else its
//! kind; a property is what a presentation app calls it; a value is said, a time in seconds. No
//! id, property path, lint code, or millisecond reaches an editor's window through here.
//!
//! The session words what it answers the editors with ([`choices`], [`state_choices`], each slot
//! of the timeline), so the browser's page keeps no vocabulary of its own (PLAN 2.33). The Mac's
//! window says the same through `Words` (`apps/mac/ScaenaKit/Sources/Scaena/Words.swift`), and
//! `tests/words.rs` holds its tables to these.

use crate::choices::{Choices, Field, StateChoices, Takes};
use crate::document::{Deck, NodeType, Props};
use crate::model::theme::{Theme, Vocabulary};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// A property as a presentation app labels it, by its path: `style/family` as `Font`.
pub const LABELS: &[(&str, &str)] = &[
    ("role", "Text Style"),
    ("emphasis", "Emphasis"),
    ("style/family", "Font"),
    ("style/weight", "Weight"),
    ("style/italic", "Italic"),
    ("style/size", "Size"),
    ("style/color", "Color"),
    ("style/case", "Capitalization"),
    ("style/tracking", "Character Spacing"),
    ("align/x", "Alignment"),
    ("fit", "Fit"),
    ("mask", "Mask"),
    ("shadow", "Shadow"),
    ("adjust/exposure", "Exposure"),
    ("adjust/contrast", "Contrast"),
    ("adjust/saturation", "Saturation"),
    ("adjust/warmth", "Warmth"),
    // A fetched map's pin and credit (ADR-0030).
    ("pin/label", "Label"),
    ("pin/color", "Pin"),
    ("credit", "Credit"),
    ("wrap", "Line Breaks"),
    ("maxLines", "Most Lines"),
    ("opacity", "Opacity"),
    ("transform/rotate", "Rotation"),
    ("enter", "Build In"),
    ("exit", "Build Out"),
    ("alt", "Description"),
    ("semantic", "Part in the Story"),
    ("focal", "Focal Point"),
    ("crop", "Crop"),
    ("radius", "Corners"),
    ("fill", "Fill"),
    ("stroke/paint", "Border"),
    ("stroke/width", "Border Width"),
    ("connect/route", "Connection"),
    ("ends/start", "Line Start"),
    ("ends/end", "Line End"),
    ("data", "Data"),
    ("kind", "Chart"),
    ("orient", "Bars Run"),
    ("x/field", "Across"),
    ("x/type", "Across Are"),
    ("y/field", "Up"),
    ("y/type", "Up Are"),
    ("series/field", "Series"),
    ("color/field", "Color By"),
    ("sizeEncoding/field", "Size By"),
    ("interval/low", "Low End"),
    ("interval/high", "High End"),
    ("facet/field", "Small Multiples By"),
    ("key", "Rows Match By"),
    ("labels/show", "Values Shown"),
    ("labels/role", "Value Style"),
    ("header", "Header Row"),
    ("preset", "Effect"),
    ("palette", "Colors"),
    ("axis", "Direction"),
    ("gap", "Spacing"),
    ("layout", "Layout"),
    ("transition/duration", "Duration"),
    ("transition/ease", "Easing"),
    ("transition/spring", "Spring"),
    ("transition/match", "Magic Move"),
    ("transition/style", "Transition"),
    ("transition/direction", "Direction"),
    ("hold", "Advance"),
    ("notes", "Presenter Notes"),
];

/// A property as the inspector labels it: from [`LABELS`], else its last key as words.
pub fn label(prop: &str) -> String {
    match LABELS.iter().find(|(p, _)| *p == prop) {
        Some((_, said)) => said.to_string(),
        None => phrase(prop.rsplit('/').next().unwrap_or(prop)),
    }
}

/// One of the words a property takes, said, by the property and the word: an image's `fit` of
/// `cover` is its Fill. A word any property takes is in [`ANY_WORDS`].
pub const WORDS: &[(&str, &str, &str)] = &[
    ("fit", "cover", "Fill"),
    ("credit", "inside", "On the Map"),
    ("credit", "below", "Below the Map"),
    ("mask", "ellipse", "Oval"),
    ("fit", "contain", "Fit"),
    ("fit", "fill", "Stretch"),
    ("fit", "wrap", "Wrap"),
    ("fit", "shrink", "Shrink to Fit"),
    ("fit", "grow", "Grow to Fill"),
    ("fit", "clip", "Cut Off"),
    ("fit", "error", "Flag It"),
    ("wrap", "greedy", "Plain"),
    ("wrap", "pretty", "Even"),
    ("wrap", "balance", "Balanced"),
    ("align/x", "start", "Left"),
    ("align/x", "center", "Center"),
    ("align/x", "end", "Right"),
    ("style/case", "none", "As Typed"),
    ("style/case", "upper", "All Caps"),
    ("style/case", "lower", "Lowercase"),
    ("style/case", "title", "Title Case"),
    ("style/case", "smallcaps", "Small Caps"),
    ("transition/match", "id", "On"),
    ("transition/match", "none", "Off"),
    ("transition/style", "dissolve", "Dissolve"),
    ("transition/style", "push", "Push"),
    ("transition/style", "cover", "Move In"),
    ("transition/style", "reveal", "Reveal"),
    ("transition/style", "wipe", "Wipe"),
    ("transition/direction", "left", "Right to Left"),
    ("transition/direction", "right", "Left to Right"),
    ("transition/direction", "up", "Bottom to Top"),
    ("transition/direction", "down", "Top to Bottom"),
    ("axis", "x", "Across"),
    ("axis", "y", "Down"),
    ("labels/show", "auto", "Automatic"),
    ("labels/show", "ends", "First and Last"),
    ("connect/route", "straight", "Straight"),
    ("connect/route", "elbow", "Corner"),
    ("connect/route", "curved", "Curved"),
    ("ends/start", "none", "None"),
    ("ends/start", "arrow", "Arrow"),
    ("ends/start", "open", "Open Arrow"),
    ("ends/start", "circle", "Circle"),
    ("ends/start", "square", "Square"),
    ("ends/start", "diamond", "Diamond"),
    ("ends/end", "none", "None"),
    ("ends/end", "arrow", "Arrow"),
    ("ends/end", "open", "Open Arrow"),
    ("ends/end", "circle", "Circle"),
    ("ends/end", "square", "Square"),
    ("ends/end", "diamond", "Diamond"),
];

/// A word said alike whatever property takes it: how a chart's channel reads its column.
pub const ANY_WORDS: &[(&str, &str)] =
    &[("quantitative", "Numbers"), ("ordinal", "In Order"), ("nominal", "Names"), ("temporal", "Dates")];

/// One of the words `prop` takes, said: from [`WORDS`] and [`ANY_WORDS`]; a data source, which
/// the deck writes `@name`, as [`source`] names it where its file is not known; else the word as
/// words.
pub fn word(prop: &str, word: &str) -> String {
    if let Some((_, _, said)) = WORDS.iter().find(|(p, w, _)| *p == prop && *w == word) {
        return said.to_string();
    }
    if let Some((_, said)) = ANY_WORDS.iter().find(|(w, _)| *w == word) {
        return said.to_string();
    }
    match word.strip_prefix('@') {
        Some(name) => source(name, None),
        None => phrase(word),
    }
}

/// Data source `name` as a person names it (PLAN 3.18), never by the id the deck writes it with:
/// by the name of its `file` in words, `data/trails-funding.csv` as Trails Funding; rows written
/// in the deck, by its own name in words.
pub fn source(name: &str, file: Option<&str>) -> String {
    let stem = file.map(|path| {
        let leaf = path.rsplit('/').next().unwrap_or(path);
        leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem)
    });
    stem.map(phrase).filter(|said| !said.is_empty()).unwrap_or_else(|| phrase(name))
}

/// A color role by what it is for.
pub const COLORS: &[(&str, &str)] = &[
    ("surface", "Background"),
    ("surface-2", "Background 2"),
    ("onSurface", "Text"),
    ("onSurfaceMuted", "Muted Text"),
    ("onAccent", "Text on Accent"),
];

/// An easing by what it does.
pub const EASINGS: &[(&str, &str)] = &[("in", "Ease In"), ("out", "Ease Out")];

/// A motion preset whose name says it in fewer words than a person would (PLAN 2.112): the
/// shipped themes' builds, as a presentation app names them. The rest are said as they read.
pub const PRESETS: &[(&str, &str)] = &[
    ("words", "Word by Word"),
    ("bullets", "By Bullet"),
    ("fadeScale", "Fade and Scale"),
    ("scaleBig", "Scale Big"),
    ("moveInLeft", "Move In from Left"),
    ("moveInRight", "Move In from Right"),
    ("moveInTop", "Move In from Top"),
    ("moveInBottom", "Move In from Bottom"),
];

/// An entrance said as it leaves, as a presentation app names its Build Out effects (PLAN 2.116):
/// an exit plays an entrance back, so what moves in from the left moves out to it, and what
/// appears disappears. One not here, Pop or Spin, is said as it enters.
pub const EXITS: &[(&str, &str)] = &[
    ("appear", "Disappear"),
    ("fade", "Fade Out"),
    ("rise", "Sink"),
    ("words", "Word by Word Out"),
    ("typewriter", "Typewriter Out"),
    ("scale", "Scale Down"),
    ("grow", "Shrink"),
    ("draw", "Erase"),
    ("moveInLeft", "Move Out to Left"),
    ("moveInRight", "Move Out to Right"),
    ("moveInTop", "Move Out to Top"),
    ("moveInBottom", "Move Out to Bottom"),
];

/// Motion preset `name` of `theme` as Build Out names it: played back, as it leaves ([`EXITS`]).
pub fn leaving(name: &str, theme: &Theme) -> String {
    EXITS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| named(Vocabulary::MotionPreset, name, theme), |(_, said)| said.to_string())
}

/// One of `theme`'s names of a kind, said: a color by what it is for, a font by its family, a
/// duration with its seconds, a step of a scale by its size, a layout by what the theme calls it.
pub fn named(of: Vocabulary, name: &str, theme: &Theme) -> String {
    let table = |table: &[(&str, &str)]| table.iter().find(|(n, _)| *n == name).map(|(_, said)| said.to_string());
    match of {
        Vocabulary::Color => table(COLORS).unwrap_or_else(|| phrase(name)),
        Vocabulary::FontFamily => {
            theme.typography.families.get(name).map_or_else(|| phrase(name), |family| family.family.clone())
        }
        Vocabulary::Duration => match theme.motion.durations.get(name) {
            Some(ms) => format!("{} ({})", phrase(name), seconds(ms.0)),
            None => phrase(name),
        },
        Vocabulary::Easing => table(EASINGS).unwrap_or_else(|| phrase(name)),
        Vocabulary::MotionPreset => table(PRESETS).unwrap_or_else(|| phrase(name)),
        Vocabulary::Radius | Vocabulary::Space => match step(theme, name) {
            Some(0.0) => "None".into(),
            Some(size) => format!("{} pt", number(size)),
            None => phrase(name),
        },
        Vocabulary::Layout => theme.layouts.get(name).and_then(|l| l.name.clone()).unwrap_or_else(|| phrase(name)),
        _ => phrase(name),
    }
}

/// A step of the theme's radius or space scale by its name, `radius.2`: its size.
fn step(theme: &Theme, name: &str) -> Option<f64> {
    let (scale, i) = name.split_once('.')?;
    let i: usize = i.parse().ok()?;
    let steps = match scale {
        "radius" => theme.tokens.radius.as_ref()?.scale.as_ref()?,
        "space" => &theme.tokens.space.scale,
        _ => return None,
    };
    steps.get(i).map(|n| n.0)
}

/// A weight by its name, as a type menu says it: 700 is Bold.
pub const WEIGHTS: &[(u16, &str)] = &[
    (100, "Thin"),
    (200, "Extra Light"),
    (300, "Light"),
    (400, "Regular"),
    (500, "Medium"),
    (600, "Semibold"),
    (700, "Bold"),
    (800, "Extra Bold"),
    (900, "Black"),
];

/// A weight said: by its name where it has one, else its number.
pub fn weight(w: f64) -> String {
    match WEIGHTS.iter().find(|(n, _)| f64::from(*n) == w) {
        Some((_, said)) => said.to_string(),
        None => number(w),
    }
}

/// A node's kind, as a person names it; none is an object.
pub fn kind(t: Option<NodeType>) -> &'static str {
    match t {
        Some(NodeType::Text) => "Text",
        Some(NodeType::Image) => "Image",
        Some(NodeType::Chart) => "Chart",
        Some(NodeType::Table) => "Table",
        Some(NodeType::Shape) => "Shape",
        Some(NodeType::Shader) => "Background Effect",
        Some(NodeType::Group | NodeType::Stack | NodeType::Grid | NodeType::Frame) => "Group",
        None => "Object",
    }
}

/// How long a name drawn from an object's words runs before it is cut.
const NAMED: usize = 40;

/// A node's kind as `props` show it, as a person names it: a shape by its kind, as Insert offers
/// it (PLAN 2.106), a polygon of the shape library by its name; a filled path on a view an Icon
/// (PLAN 2.104, [`crate::inserts::is_icon`]), and the pen's, stroked, a Drawing (PLAN 2.114); a
/// container with a panel of its own a Card; and anything else its type's kind.
pub fn kind_of(t: NodeType, props: &Props) -> &'static str {
    match t {
        NodeType::Shape => {
            let path = props.contains_key("path");
            match props.get("kind").and_then(Value::as_str).unwrap_or(if path { "path" } else { "rect" }) {
                "path" if crate::inserts::is_icon(|key| props.get(key)) => "Icon",
                "path" => "Drawing",
                "polygon" => library(props).unwrap_or("Polygon"),
                other => SHAPES.iter().find(|(k, _)| *k == other).map_or("Shape", |(_, said)| said),
            }
        }
        NodeType::Stack | NodeType::Grid | NodeType::Frame if props.contains_key("fill") => "Card",
        _ => kind(Some(t)),
    }
}

/// The shape library's name for a polygon whose points, as `props` show them, are one of its
/// polygons' (PLAN 2.106), said: a Star.
fn library(props: &Props) -> Option<&'static str> {
    let points: Vec<[f64; 2]> = serde_json::from_value(props.get("points")?.clone()).ok()?;
    let same = |theirs: &[[f64; 2]]| {
        theirs.len() == points.len()
            && theirs.iter().zip(&points).all(|(a, b)| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3)
    };
    let (name, ..) = crate::inserts::polygons().into_iter().find(|(_, theirs, _)| same(theirs))?;
    SHAPES.iter().find(|(k, _)| *k == name).map(|(_, said)| *said)
}

/// A node as a person names it, as `props` show it: a text by its words, anything else by its
/// description, the first line of either, cut at 40 characters; else its kind.
pub fn object(t: NodeType, props: &Props) -> String {
    let words = match t {
        NodeType::Text => crate::reading::words(props),
        _ => props.get("alt").and_then(Value::as_str).unwrap_or_default().to_string(),
    };
    let line = words.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return kind_of(t, props).to_string();
    }
    cut(line)
}

/// A first line, cut at 40 characters.
fn cut(line: &str) -> String {
    if line.chars().count() > NAMED {
        let cut: String = line.chars().take(NAMED - 1).collect();
        return format!("{}…", cut.trim_end());
    }
    line.to_string()
}

/// Node `id` as a person names it where `shown` shows it (a state's nodes, in the deck's order),
/// as [`object`] names it; but a shape or a container with no description of its own by the
/// words it holds: the first text in it, or in what it holds, as a card is named by its heading
/// and a shape by the words typed in it (PLAN 2.108).
pub fn object_in(deck: &Deck, shown: &indexmap::IndexMap<String, Props>, id: &str) -> String {
    let Some(node) = deck.nodes.get(id) else { return kind(None).to_string() };
    let props = shown.get(id).unwrap_or(&node.props);
    let said = object(node.node_type, props);
    let holds = !matches!(node.node_type, NodeType::Text | NodeType::Image | NodeType::Chart | NodeType::Table);
    if !holds || said != kind_of(node.node_type, props) {
        return said;
    }
    held_words(deck, shown, id, 0).map_or(said, |words| cut(&words))
}

/// The first line of the first text `shown` holds in `holder`, at any depth, in the deck's order.
fn held_words(deck: &Deck, shown: &indexmap::IndexMap<String, Props>, holder: &str, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let held =
        shown.iter().filter(|(_, p)| p.get("at").and_then(|a| a.get("parent")).and_then(Value::as_str) == Some(holder));
    for (id, props) in held {
        let words = match deck.nodes.get(id).map(|n| n.node_type) {
            Some(NodeType::Text) => crate::reading::words(props),
            _ => held_words(deck, shown, id, depth + 1).unwrap_or_default(),
        };
        let line = words.lines().next().unwrap_or_default().trim();
        if !line.is_empty() {
            return Some(line.to_string());
        }
    }
    None
}

/// Where each of `deck`'s states stands among its slides, as a person counts them, in the deck's
/// order: `Slide 3`, or `Slide 3, step 2` for a step after a slide's first.
pub fn slides(deck: &Deck) -> Vec<String> {
    let mut out = Vec::with_capacity(deck.states.len());
    let (mut number, mut step) = (0, 0);
    let mut slide: Option<&str> = None;
    for state in &deck.states {
        let of = deck.slide_of(state);
        if slide != Some(of) {
            number += 1;
            step = 1;
            slide = Some(of);
        } else {
            step += 1;
        }
        out.push(if step > 1 { format!("Slide {number}, step {step}") } else { format!("Slide {number}") });
    }
    out
}

/// Each of `deck`'s slides as it stands once its steps have played: its number, as [`slides`]
/// counts it, and its last state, in the deck's order. What a picture of every slide shows, a
/// file to a slide (PLAN 2.122).
pub fn last_steps(deck: &Deck) -> Vec<(usize, &str)> {
    let mut out: Vec<(usize, &str)> = Vec::new();
    let mut slide: Option<&str> = None;
    for state in &deck.states {
        let of = deck.slide_of(state);
        match out.last_mut() {
            Some(last) if slide == Some(of) => last.1 = &state.id,
            _ => out.push((out.len() + 1, &state.id)),
        }
        slide = Some(of);
    }
    out
}

/// `state` as [`slides`] numbers it: `Slide 3, step 2`; a state the deck has not, `Slide`.
pub fn slide(deck: &Deck, state: &str) -> String {
    match deck.states.iter().position(|s| s.id == state) {
        Some(i) => slides(deck).swap_remove(i),
        None => "Slide".into(),
    }
}

/// What each operation a patch made did, by its name, as a change of the history says it.
const OPS: &[(&str, &str)] = &[
    ("add_node", "an object added"),
    ("remove_node", "an object deleted"),
    ("rename_node", "an object renamed"),
    ("set_prop", "an object changed"),
    ("set_text", "words changed"),
    ("replace_text", "words typed"),
    ("style_text", "characters given a look"),
    ("list", "a list made"),
    ("choose", "a look chosen"),
    ("place", "an object moved"),
    ("hide_node", "an object hidden"),
    ("show_node", "an object shown"),
    ("annotate", "a chart annotated"),
    ("group", "objects grouped"),
    ("ungroup", "a group taken apart"),
    ("apply_preset", "an animation added"),
    ("time_motion", "an animation timed"),
    ("bind_data", "a chart given data"),
    ("add_state", "a slide added"),
    ("set_state", "a slide changed"),
    ("move_state", "a slide moved"),
    ("rename_state", "a slide renamed"),
    ("remove_state", "a slide deleted"),
    ("duplicate_slide", "a slide copied"),
    ("move_slide", "a slide moved"),
    ("remove_slide", "a slide deleted"),
    ("retheme", "the theme changed"),
];

/// What a change the deck's history keeps did, from the message it was recorded with, as a
/// person says it (PLAN 2.99): `patch: add_node, place` as An object added, an object moved; a
/// theme taken by its name; a data source by its name in words, never the id the deck writes it
/// with (PLAN 3.18); an undo by what it undid. Every editor's Versions lists a change so.
pub fn change(message: &str) -> String {
    let m = message.trim();
    let file = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
    if let Some(ops) = m.strip_prefix("patch: ") {
        let mut said: Vec<&str> = Vec::new();
        for op in ops.split(", ") {
            let did = OPS.iter().find(|(name, _)| *name == op).map_or("an edit", |(_, did)| did);
            if !said.contains(&did) {
                said.push(did);
            }
        }
        return sentence(&said.join(", "));
    }
    if let Some((how, rest)) = m.split_once(' ').filter(|(how, _)| matches!(*how, "undo" | "redo")) {
        let done = if how == "undo" { "Undone" } else { "Made again" };
        return format!("{done}: {}", change(rest).to_lowercase());
    }
    match m {
        "" => return "A change".into(),
        "history begins" => return "The history begins".into(),
        "type" => return "Typing".into(),
        "edit" => return "Edits".into(),
        "save" => return "Saved".into(),
        "deck_create" => return "The deck made".into(),
        _ => {}
    }
    if let Some(name) = m.strip_prefix("data_attach ") {
        return format!("The data “{}” added", source(name, None));
    }
    if let Some(name) = m.strip_prefix("data_edit ") {
        return format!("The data “{}” changed", source(name.split(':').next().unwrap_or(name), None));
    }
    if m.starts_with("history --restore ") {
        return "A version restored".into();
    }
    if let Some(path) = m.strip_prefix("theme --apply ") {
        let name = file(path);
        let name = name.strip_suffix(".theme.json").or_else(|| name.strip_suffix(".json")).unwrap_or(&name);
        return format!("The theme {} taken", phrase(name));
    }
    if let Some(path) = m.strip_prefix("theme_edit: colors from ") {
        return format!("The theme given the colors of {}", file(path));
    }
    if m.starts_with("theme_edit") {
        return "The theme changed".into();
    }
    if m.starts_with("lint --fix") {
        return "Issues fixed".into();
    }
    if m.starts_with("library: ") {
        return "The theme's layouts brought up to date".into();
    }
    if m.ends_with("changed outside Scaena") {
        return "Changed outside Scaena".into();
    }
    sentence(m)
}

/// Who made a change the deck's history keeps, as a person reads it (PLAN 2.99): `you`, `the
/// assistant` and its model, or an edit made outside Scaena; another author by the name the
/// history keeps.
pub fn author(author: &str) -> String {
    match author {
        "" | "user" => "you".into(),
        "fs" => "outside Scaena".into(),
        _ => match author.strip_prefix("agent:") {
            Some("") => "the assistant".into(),
            Some(model) => format!("the assistant ({model})"),
            None => author.into(),
        },
    }
}

/// `words` begun with a capital.
fn sentence(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// What a motion does, as a person says it: `enter` as Enters, and one split by words as Enters
/// Word by Word.
pub fn motion(kind: &str, split: Option<&str>) -> String {
    let does = match kind {
        "enter" => "Enters".to_string(),
        "exit" => "Leaves".to_string(),
        "emphasis" => "Emphasis".to_string(),
        "anim" => "Moves".to_string(),
        other => phrase(other),
    };
    let by = match split {
        Some("lines") => "Line by Line",
        Some("words") => "Word by Word",
        Some("glyphs") => "Letter by Letter",
        Some("paragraphs") => "Bullet by Bullet",
        Some("children") => "One by One",
        Some("marks") => "Mark by Mark",
        _ => return does,
    };
    format!("{does} {by}")
}

/// A shape's kind, as Insert offers it, and the shape library's polygons by name (PLAN 2.106).
pub const SHAPES: &[(&str, &str)] = &[
    ("rect", "Rectangle"),
    ("ellipse", "Oval"),
    ("line", "Line"),
    ("arrow", "Arrow"),
    ("polygon", "Polygon"),
    ("triangle", "Triangle"),
    ("right-triangle", "Right Triangle"),
    ("diamond", "Diamond"),
    ("parallelogram", "Parallelogram"),
    ("trapezoid", "Trapezoid"),
    ("pentagon", "Pentagon"),
    ("hexagon", "Hexagon"),
    ("octagon", "Octagon"),
    ("star", "Star"),
    ("chevron", "Chevron"),
    ("right-arrow", "Right Arrow"),
    ("left-arrow", "Left Arrow"),
    ("up-arrow", "Up Arrow"),
    ("down-arrow", "Down Arrow"),
    ("plus", "Plus"),
    ("callout", "Callout"),
];

/// A day, `YYYY-MM-DD`, as a person says it: "15 October 2026"; anything else as it is written.
pub fn day(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let part = |r: std::ops::Range<usize>| date.get(r).and_then(|p| p.parse::<usize>().ok());
    match (part(0..4), part(5..7), part(8..10)) {
        (Some(y), Some(m @ 1..=12), Some(d)) if date.len() == 10 => format!("{d} {} {y}", MONTHS[m - 1]),
        _ => date.to_string(),
    }
}

/// A shape's kind said: `rect` as Rectangle.
pub fn shape(kind: &str) -> String {
    SHAPES.iter().find(|(k, _)| *k == kind).map_or_else(|| phrase(kind), |(_, said)| said.to_string())
}

/// What lint finds that a person can put right on a slide, said in a sentence, by its code: what
/// an editor marks on its object on the canvas (PLAN 3.20).
pub const MARKED: &[(&str, &str)] = &[
    ("E100", "The text doesn't fit its box."),
    ("E101", "It overlaps another object."),
    ("E110", "It's hard to read against what's behind it."),
    ("E111", "It's hard to read against what's behind it."),
    ("E120", "Some of its characters aren't in its font."),
    ("W202", "It runs to more lines than its style allows."),
    ("W203", "It doesn't fit, even at its smallest size."),
    ("W205", "The code is too small to read."),
    ("W231", "Its font has no italic, so it's set upright."),
    ("W232", "Its font has no bold, so it's set in its own weight."),
    ("W303", "The slide's number or footer is drawn over it."),
    ("W310", "Some of the chart's labels overlap."),
    ("W311", "A background effect is behind this chart."),
    ("W312", "Some of the chart's text is too small to read."),
    ("W313", "The chart is too small to read."),
    ("W314", "A credit here can't be read."),
    ("W410", "The picture has no description for people who can't see it."),
    ("W427", "A figure here no longer matches its data."),
    ("I404", "This haiku's lines don't count 5, 7, and 5."),
];

/// The rest of what lint finds, said in a sentence, by its code (PLAN 2.99): the deck's structure,
/// its story, its motion, and its placement, which an editor lists rather than marks.
pub const LISTED: &[(&str, &str)] = &[
    ("E102", "It names something the deck or its theme doesn't have."),
    ("E103", "Its data doesn't have what it reads."),
    (
        "E104",
        "It's set up in a way the deck doesn't allow: its kind changes, a slide locks it, or a slide changes what it joins.",
    ),
    ("E105", "A name is used twice, or isn't one the deck allows."),
    ("E106", "Something in it isn't written the way the deck allows."),
    ("W200", "Its last line is left too short."),
    ("W201", "A word is too long to fit its line."),
    ("W210", "This slide has more words than the theme allows."),
    ("W220", "Paragraphs on this slide are aligned more than one way."),
    ("W221", "A text style's line spacing is off the theme's baseline grid."),
    ("W230", "A font's license may not allow the deck to carry it."),
    ("W300", "It's set in a value of its own where the theme has a name for one."),
    ("W301", "It's placed off the theme's grid."),
    ("W302", "It's placed where the deck's other sizes won't move it."),
    ("W320", "Too many things move at once."),
    ("W321", "This slide's animations run too long."),
    ("W322", "An animation here moves nothing."),
    ("W323", "This slide shows for no time at all in a deck that plays on its own."),
    ("W401", "This slide isn't part of the story's outline."),
    ("W420", "This part of the story shows no claim."),
    ("W421", "The slide ends on evidence with no claim."),
    ("W422", "Evidence is set larger than its claim."),
    ("W423", "This part of the story cites data or a picture it never shows."),
    ("W424", "More than one claim is on screen at once."),
    ("W425", "Two parts of the story in a row make the same claim."),
    ("W426", "This part of the story plays out of the outline's order."),
    ("I400", "This step is the same as the one before it."),
    ("I401", "It never shows on any slide."),
    ("I402", "Settings of its own keep a theme change from reaching it."),
    ("I403", "Its code is in a language Scaena colors only by its strings, numbers, and comments."),
];

/// What lint finds, said in a sentence, by its code: what an editor marks ([`MARKED`]), then what
/// it lists ([`LISTED`]). None for a code lint does not have.
pub fn issue(code: &str) -> Option<&'static str> {
    MARKED.iter().chain(LISTED).find(|(c, _)| *c == code).map(|(_, said)| *said)
}

/// What lint finds, said in a sentence, from the finding itself where its measure says more than
/// its code: a haiku's counts (I404, PLAN O.4h), "This haiku's lines count 5, 8, and 5, not 5,
/// 7, and 5."; else [`issue`].
pub fn said(code: &str, measure: Option<&serde_json::Value>) -> Option<String> {
    if code == "I404"
        && let Some(counts) = measure.and_then(|m| m.get("counts")).and_then(serde_json::Value::as_array)
        && counts.len() == 3
    {
        let said: Vec<String> = counts
            .iter()
            .map(|c| {
                match (c.get(0).and_then(serde_json::Value::as_u64), c.get(1).and_then(serde_json::Value::as_u64)) {
                    (Some(low), Some(high)) if low != high => format!("{low} or {high}"),
                    (Some(low), _) => low.to_string(),
                    _ => "?".to_string(),
                }
            })
            .collect();
        return Some(format!("This haiku's lines count {}, {}, and {}, not 5, 7, and 5.", said[0], said[1], said[2]));
    }
    issue(code).map(str::to_string)
}

/// Whether an editor marks what lint finds by `code` on its object on the canvas.
pub fn marked(code: &str) -> bool {
    MARKED.iter().any(|(c, _)| *c == code)
}

/// A fix as its button names it, by the code of what it fixes.
pub fn fixing(code: &str) -> &'static str {
    match code {
        "E110" | "E111" => "Use a Color That Reads",
        "E100" | "W202" => "Shrink to Fit",
        "W427" => "Update the Figure",
        "W314" => "Put the Credit Below",
        _ => "Fix",
    }
}

/// A theme file as a person names its theme: `themes/dusk.theme.json` as `Dusk`.
pub fn theme(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    let name = name.strip_suffix(".json").unwrap_or(name);
    phrase(name.strip_suffix(".theme").unwrap_or(name))
}

/// Milliseconds said in seconds: `5.5 s`, `0.18 s`.
pub fn seconds(ms: f64) -> String {
    format!("{} s", number(ms / 1000.0))
}

/// A number as a person writes it: whole where it is, else to two places, no trailing zeros.
pub fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e12 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.to_string() }
}

/// A name as words: `onSurfaceMuted` as `On Surface Muted`, `art-left` as `Art Left`,
/// `stackedBar` as `Stacked Bar`, `radius.2` as `Radius 2`.
pub fn phrase(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    for ch in name.chars() {
        if matches!(ch, '-' | '_' | '.' | ' ') {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else if ch.is_uppercase() && word.chars().last().is_some_and(|last| last.is_lowercase() || last.is_numeric())
        {
            words.push(std::mem::take(&mut word));
            word.push(ch);
        } else {
            word.push(ch);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let capital = |w: &String| {
        let mut chars = w.chars();
        chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
    };
    words.iter().map(capital).collect::<Vec<_>>().join(" ")
}

/// A value typed in a data source's cell that its column does not read, in a sentence (PLAN
/// 2.99), the column named as the sheet heads it: The revenue column takes a number: “four” is
/// not one.
pub fn unfit(column: &str, kind: crate::data::ColumnType, value: &str) -> String {
    use crate::data::ColumnType;
    let typed = value.trim();
    match kind {
        ColumnType::Number => format!("The {column} column takes a number: “{typed}” is not one"),
        ColumnType::Date => format!("The {column} column takes a date: “{typed}” is not one it reads"),
        ColumnType::Boolean => format!("The {column} column takes true or false: “{typed}” is neither"),
        ColumnType::String => format!("The {column} column does not take “{typed}”"),
    }
}

/// A property's words, as an editor shows them (PLAN 2.99): its label, and each name or word it
/// offers, the value, and the theme's, said.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldWords {
    /// The property as a presentation app labels it: `Font`, `Build In`, `Advance`.
    pub label: String,
    /// Each name or word offered, and the value and the theme's where they are one, said:
    /// `onSurface` as `Text`, `fast` as `Fast (0.18 s)`, `cover` as `Fill`; a weight by its name.
    pub said: BTreeMap<String, String>,
    /// For a color, each of the theme's names offered, and the value and the theme's where they
    /// are one, as the color it is, `#rrggbb`: what a color well shows beside the name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
}

/// The color `name` is in `theme`, `#rrggbb` (its alpha left off): a color role through the
/// token it names, a token, or a color written out. None for a name the theme lacks, or a
/// gradient.
pub fn hex(name: &str, theme: &Theme) -> Option<String> {
    let tokens = &theme.tokens;
    let literal = match name.starts_with('#') || name.starts_with("oklch(") || name.starts_with("oklab(") {
        true => name,
        false => &tokens.color.get(name).or_else(|| tokens.color.get(tokens.roles.get(name)?))?.0,
    };
    let [r, g, b, _] = crate::color::parse(literal).ok()?.0;
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// `field`'s words, its names said as `theme` has them.
pub fn field(field: &Field, theme: &Theme) -> FieldWords {
    let say = |key: &str| match &field.takes {
        // Build Out offers the entrances played back, each named as it leaves.
        Takes::Name { of: Vocabulary::MotionPreset, .. } if field.prop == "exit" => leaving(key, theme),
        Takes::Name { of, .. } => named(*of, key, theme),
        _ => word(&field.prop, key),
    };
    let offered: &[String] = match &field.takes {
        Takes::Name { names, .. } => names,
        Takes::Word { words } => words,
        _ => &[],
    };
    let mut said = BTreeMap::new();
    for key in offered {
        said.insert(key.clone(), say(key));
    }
    let shown = [&field.value, &field.theme];
    if !offered.is_empty() || matches!(field.takes, Takes::Name { .. } | Takes::Word { .. }) {
        for value in shown.into_iter().flatten() {
            // A value written out (a color, `#2d6cdf`) is said as it is written.
            if let Value::String(s) = value
                && !s.starts_with('#')
                && !s.contains('(')
            {
                said.insert(s.clone(), say(s));
            }
        }
    }
    // A color well shows the color each name is, and the value's and the theme's.
    let mut colors = BTreeMap::new();
    if let Takes::Name { of: Vocabulary::Color, names, .. } = &field.takes {
        let values = shown.into_iter().flatten().filter_map(Value::as_str);
        for name in names.iter().map(String::as_str).chain(values) {
            if let Some(hex) = hex(name, theme) {
                colors.insert(name.to_string(), hex);
            }
        }
    }
    if field.prop == "style/weight" {
        for (w, name) in WEIGHTS {
            said.insert(w.to_string(), name.to_string());
        }
        for value in shown.into_iter().flatten() {
            if let Some(w) = value.as_f64() {
                said.insert(value.to_string(), weight(w));
            }
        }
    }
    FieldWords { label: label(&field.prop), said, colors }
}

/// `choices` worded: the node named by what it shows (`said`) and by its kind (`noun`), each field
/// labeled and its values said, in `theme`.
pub fn choices(choices: &mut Choices, deck: &Deck, theme: &Theme) {
    let shown = crate::tracking::resolve_states(deck)
        .ok()
        .and_then(|snaps| snaps.into_iter().find(|s| s.state_id == choices.state))
        .map(|snap| snap.nodes)
        .filter(|nodes| nodes.contains_key(&choices.node));
    choices.said = Some(match &shown {
        Some(nodes) => object_in(deck, nodes, &choices.node),
        None => kind(Some(choices.node_type)).to_string(),
    });
    // Its kind as the state shows it: a shape by its kind, and a filled path on a view an Icon
    // (PLAN 2.104), as Insert makes one, which the picker replaces; the pen's, stroked, is a
    // Drawing (PLAN 2.114).
    let props = shown.as_ref().and_then(|nodes| nodes.get(&choices.node));
    choices.noun = Some(props.map_or(kind(Some(choices.node_type)), |p| kind_of(choices.node_type, p)).to_string());
    if choices.node_type == NodeType::Image {
        choices.framed = frame(theme);
    }
    let chart =
        props.and_then(|p| p.get("kind")).and_then(Value::as_str).filter(|_| choices.node_type == NodeType::Chart);
    let horizontal = props.and_then(|p| p.get("orient")).and_then(Value::as_str) == Some("horizontal");
    for f in &mut choices.fields {
        let mut words = field(f, theme);
        if let Some(said) = channel(&f.prop, chart, horizontal) {
            words.label = said.to_string();
        }
        // A chart's or a table's sources, each by the file the deck declares it in (PLAN 3.18).
        if f.prop == "data" {
            for (written, said) in &mut words.said {
                if let Some(name) = written.strip_prefix('@') {
                    *said = source(name, deck.data.get(name).and_then(|d| d.source.as_str()));
                }
            }
        }
        f.words = Some(words);
    }
}

/// How `theme` frames its pictures, said (PLAN T.3a): `A 24 pt mat in Text and a 2 pt keyline in
/// Accent, the theme's`; none where it frames none.
pub fn frame(theme: &Theme) -> Option<String> {
    let (keyline, mat) = theme.pictures.as_ref()?.frame.bands();
    let band = |b: Option<&crate::model::theme::Band>, what: &str| {
        b.map(|b| format!("a {} pt {what} in {}", number(b.width), named(Vocabulary::Color, &b.color, theme)))
    };
    let bands: Vec<String> = [band(mat, "mat"), band(keyline, "keyline")].into_iter().flatten().collect();
    let first = bands.first()?;
    let said = format!("{}{}{}", first[..1].to_uppercase(), &first[1..], bands.get(1).map_or(String::new(), |b| format!(" and {b}")));
    Some(format!("{said}, the theme's"))
}

/// A chart's channel as a presentation app labels it on a chart of `kind`: a donut's categories
/// are its Slices and their sizes its Values, as the Mac says them; bars that run across name
/// their categories down the side and their values across (PLAN 1.29). None where [`LABELS`]
/// says it.
pub fn channel(prop: &str, kind: Option<&str>, horizontal: bool) -> Option<&'static str> {
    let horizontal = horizontal && matches!(kind, Some("bar" | "stackedBar"));
    CHANNELS
        .iter()
        .find(|(k, p, _)| *p == prop && (*k == kind.unwrap_or_default() || (*k == "across" && horizontal)))
        .map(|(_, _, said)| *said)
}

/// A chart's channel labeled by its kind ([`channel`]): the kind (or `across`, bars that run
/// across), the property, and its label.
pub const CHANNELS: &[(&str, &str, &str)] = &[
    ("donut", "x/field", "Slices"),
    ("donut", "x/type", "Slices Are"),
    ("donut", "y/field", "Values"),
    ("donut", "y/type", "Values Are"),
    ("across", "x/field", "Down the Side"),
    ("across", "x/type", "Down the Side Are"),
    ("across", "y/field", "Across"),
    ("across", "y/type", "Across Are"),
];

/// `choices` of a state worded: the state as its slide's number says it (`said`), and each field
/// labeled and its values said, in `theme`.
pub fn state_choices(choices: &mut StateChoices, deck: &Deck, theme: &Theme) {
    choices.said = Some(slide(deck, &choices.state));
    for f in &mut choices.fields {
        f.words = Some(field(f, theme));
    }
}

/// Every kind of name a theme defines, in the order an editor lists them.
const VOCABULARIES: [Vocabulary; 15] = [
    Vocabulary::TextRole,
    Vocabulary::FontFamily,
    Vocabulary::Color,
    Vocabulary::Layout,
    Vocabulary::MotionPreset,
    Vocabulary::Duration,
    Vocabulary::Easing,
    Vocabulary::Spring,
    Vocabulary::ShaderPreset,
    Vocabulary::ShaderPalette,
    Vocabulary::DataPalette,
    Vocabulary::Stroke,
    Vocabulary::Shadow,
    Vocabulary::Radius,
    Vocabulary::Space,
];

/// The words an editor shows for a state and the deck around it (PLAN 2.99): each object the
/// state shows named by what it shows and by its kind, each of the theme's names said, how a
/// motion, its split, a shape, and each issue lint finds are said, and each data source named.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lexicon {
    /// Each node, by its id: what it shows, as the state shows it where it does, else its kind
    /// ([`object_in`]).
    pub objects: BTreeMap<String, String>,
    /// Each node, by its id: its kind ([`kind_of`]).
    pub kinds: BTreeMap<String, String>,
    /// Each of the theme's names said ([`named`]), by its kind of name: `color`, `text-role`,
    /// `layout`, `motion-preset`, … and `motion-exit`, each preset as Build Out names it.
    pub names: BTreeMap<String, BTreeMap<String, String>>,
    /// What a motion does: `enter` is Enters.
    pub motions: BTreeMap<String, String>,
    /// What a motion moves one at a time: `words` is Word by Word.
    pub splits: BTreeMap<String, String>,
    /// A shape's kind: `rect` is Rectangle.
    pub shapes: BTreeMap<String, String>,
    /// Each place a node is put by name, said: a slot of the theme's layouts, and an area of a
    /// grid the state's nodes stand in (`kicker` is Kicker).
    pub places: BTreeMap<String, String>,
    /// Each weight a type menu names, by its number: `700` is Bold.
    pub weights: BTreeMap<String, String>,
    /// Each type of node, by its name in the format: `shader` is Background Effect.
    pub types: BTreeMap<String, String>,
    /// What lint finds, by its code, in a sentence a person would say ([`issue`]): `E100` is The
    /// text doesn't fit its box. An editor says by these a finding that came without its
    /// sentence, as those do that refuse a theme, a version, or a data edit.
    pub issues: BTreeMap<String, String>,
    /// Each data source the deck declares, by the name the deck writes it with, as a person names
    /// it ([`source`], PLAN 3.18): `q3`, read from `data/q3-revenue.csv`, is Q3 Revenue. An editor
    /// names a source by these wherever it offers one or speaks of it: Insert's charts and tables,
    /// the rows a mark is drawn from, the file a source reads.
    pub sources: BTreeMap<String, String>,
}

/// Every type of node, in the format's order.
const TYPES: [NodeType; 10] = [
    NodeType::Text,
    NodeType::Shape,
    NodeType::Image,
    NodeType::Chart,
    NodeType::Table,
    NodeType::Shader,
    NodeType::Stack,
    NodeType::Grid,
    NodeType::Frame,
    NodeType::Group,
];

/// The words for `state` of `deck` in `theme`: each node as the state shows it, or, where it shows
/// it not, as the node itself is.
pub fn lexicon(deck: &Deck, theme: &Theme, state: &str) -> Result<Lexicon, String> {
    let snaps = crate::tracking::resolve_states(deck).map_err(|e| e.to_string())?;
    let snap = snaps.iter().find(|s| s.state_id == state).ok_or_else(|| format!("no state `{state}`"))?;
    // Each node by what the state shows of it; one it does not show, as the node itself is.
    let (mut objects, mut kinds) = (BTreeMap::new(), BTreeMap::new());
    for (id, node) in &deck.nodes {
        let props = snap.nodes.get(id).unwrap_or(&node.props);
        objects.insert(id.clone(), object_in(deck, &snap.nodes, id));
        kinds.insert(id.clone(), kind_of(node.node_type, props).to_string());
    }
    let mut names = BTreeMap::new();
    for of in VOCABULARIES {
        let key = serde_json::to_value(of).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let mut said = BTreeMap::new();
        for name in theme.names(of) {
            let words = named(of, &name, theme);
            said.insert(name, words);
        }
        names.insert(key, said);
    }
    // Each motion preset as Build Out names it, played back as it leaves (PLAN 2.116).
    let exits = theme.names(Vocabulary::MotionPreset).into_iter().map(|n| (leaving(&n, theme), n));
    names.insert("motion-exit".to_string(), crate::sort::map(exits.map(|(said, name)| (name, said))));
    // A slide transition's style and the way it moves, as the inspector says them (PLAN 2.111).
    for prop in ["transition/style", "transition/direction"] {
        let mut said = BTreeMap::new();
        for (_, word, words) in WORDS.iter().filter(|(p, _, _)| *p == prop) {
            said.insert(word.to_string(), words.to_string());
        }
        names.insert(prop.to_string(), said);
    }
    let mut motions = BTreeMap::new();
    for kind in ["enter", "exit", "emphasis", "anim"] {
        motions.insert(kind.to_string(), motion(kind, None));
    }
    let mut splits = BTreeMap::new();
    for split in ["lines", "words", "glyphs", "paragraphs", "children", "marks"] {
        let by = motion("", Some(split));
        splits.insert(split.to_string(), by.trim().to_string());
    }
    let mut shapes = BTreeMap::new();
    for (kind, said) in SHAPES {
        shapes.insert(kind.to_string(), said.to_string());
    }
    let mut places = BTreeMap::new();
    let slots = theme.layouts.values().flat_map(|l| {
        let formats = l.formats.iter().flat_map(|f| f.values()).flat_map(|f| f.slots.keys());
        l.slots.keys().chain(formats)
    });
    let areas = snap.nodes.values().filter_map(|props| props.get("at")?.get("area")?.as_str());
    for place in slots.map(String::as_str).chain(areas) {
        if !places.contains_key(place) {
            places.insert(place.to_string(), phrase(place));
        }
    }
    let mut weights = BTreeMap::new();
    for (w, said) in WEIGHTS {
        weights.insert(w.to_string(), said.to_string());
    }
    let mut types = BTreeMap::new();
    for t in TYPES {
        let name = serde_json::to_value(t).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        types.insert(name, kind(Some(t)).to_string());
    }
    let mut issues = BTreeMap::new();
    for (code, said) in MARKED.iter().chain(LISTED) {
        issues.insert(code.to_string(), said.to_string());
    }
    let mut sources = BTreeMap::new();
    for (name, data) in &deck.data {
        sources.insert(name.clone(), source(name, data.source.as_str()));
    }
    Ok(Lexicon { objects, kinds, names, motions, splits, shapes, places, weights, types, issues, sources })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture's frame said as the inspector shows it (PLAN T.3a): the mat first, by its width
    /// and its color as the theme's names say it; none where the theme frames none.
    #[test]
    fn a_frame_reads_as_its_bands() {
        const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");
        let dusk = Theme::from_json(DUSK).unwrap();
        assert_eq!(frame(&dusk), None, "Dusk frames none");
        let (accent, surface) = (named(Vocabulary::Color, "accent", &dusk), named(Vocabulary::Color, "surface", &dusk));
        let mut raw: Value = serde_json::from_str(DUSK).unwrap();
        let mut framed = |frame: Value| {
            raw["pictures"] = serde_json::json!({ "frame": frame });
            super::frame(&Theme::from_value(&raw).unwrap())
        };
        let both = framed(serde_json::json!({
            "keyline": { "color": "accent", "width": 2 }, "mat": { "color": "surface", "width": 24 }
        }));
        assert_eq!(both.unwrap(), format!("A 24 pt mat in {surface} and a 2 pt keyline in {accent}, the theme's"));
        let line = framed(serde_json::json!({ "keyline": { "color": "accent", "width": 1.5 } }));
        assert_eq!(line.unwrap(), format!("A 1.5 pt keyline in {accent}, the theme's"));
    }

    #[test]
    fn a_name_reads_as_words() {
        assert_eq!(phrase("onSurfaceMuted"), "On Surface Muted");
        assert_eq!(phrase("art-left"), "Art Left");
        assert_eq!(phrase("stackedBar"), "Stacked Bar");
        assert_eq!(phrase("radius.2"), "Radius 2");
        assert_eq!(phrase("q3_revenue"), "Q3 Revenue");
        assert_eq!(phrase("h2Title"), "H2 Title");
        assert_eq!(phrase(""), "");
    }

    #[test]
    fn each_slide_ends_at_its_last_step() {
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        // `mix` is the revenue slide's second step.
        assert_eq!(slides(&deck), ["Slide 1", "Slide 2", "Slide 2, step 2", "Slide 3"]);
        assert_eq!(last_steps(&deck), [(1, "intro"), (2, "mix"), (3, "close")]);
    }

    #[test]
    fn a_number_is_written_as_a_person_writes_it() {
        assert_eq!(number(5.0), "5");
        assert_eq!(number(0.18), "0.18");
        assert_eq!(number(1.5), "1.5");
        assert_eq!(number(1.005), "1");
        assert_eq!(number(-0.001), "0");
        assert_eq!(seconds(5500.0), "5.5 s");
        assert_eq!(seconds(180.0), "0.18 s");
        assert_eq!(seconds(1920.0), "1.92 s");
    }

    #[test]
    fn a_property_is_labeled_as_a_presentation_app_labels_it() {
        assert_eq!(label("style/family"), "Font");
        assert_eq!(label("hold"), "Advance");
        assert_eq!(label("transition/match"), "Magic Move");
        assert_eq!(label("labels/collide"), "Collide", "else its last key as words");
        assert_eq!(word("fit", "cover"), "Fill");
        assert_eq!(word("x/type", "temporal"), "Dates");
        assert_eq!(word("data", "@q3"), "Q3", "a data source by its name in words, not its id");
        assert_eq!(word("semantic", "takeaway"), "Takeaway");
        assert_eq!(motion("enter", Some("words")), "Enters Word by Word");
        assert_eq!(motion("exit", None), "Leaves");
        assert_eq!(weight(700.0), "Bold");
        assert_eq!(weight(650.0), "650");
        assert_eq!(theme("themes/dusk.theme.json"), "Dusk");
        assert_eq!(theme("themes/dusk-2.theme.json"), "Dusk 2");
    }

    fn props(v: serde_json::Value) -> Props {
        serde_json::from_value(v).unwrap()
    }

    /// A shape is named by its kind as Insert offers it (PLAN 2.106), a polygon of the library by
    /// its name; a path on a view is an Icon, and the pen's stroked path a Drawing (PLAN 2.114); a
    /// container with a panel of its own is a Card.
    #[test]
    fn a_shape_is_named_by_its_kind() {
        let star = crate::inserts::polygons().into_iter().find(|(name, ..)| *name == "star").unwrap().1;
        let shape = |v| kind_of(NodeType::Shape, &props(v));
        assert_eq!(shape(serde_json::json!({ "kind": "rect" })), "Rectangle");
        assert_eq!(shape(serde_json::json!({})), "Rectangle", "a shape's kind is a rectangle where it says none");
        assert_eq!(shape(serde_json::json!({ "kind": "ellipse" })), "Oval");
        assert_eq!(shape(serde_json::json!({ "kind": "polygon", "points": star })), "Star");
        assert_eq!(shape(serde_json::json!({ "kind": "polygon", "points": [[0, 0], [1, 0], [0.5, 0.4]] })), "Polygon");
        assert_eq!(shape(serde_json::json!({ "kind": "line", "connect": { "from": "a", "to": "b" } })), "Line");
        let icon = crate::inserts::icon_node("rocket", "M0 0L1 1Z", [0.0, 0.0, 256.0, 256.0], "accent");
        assert_eq!(kind_of(NodeType::Shape, &props(icon)), "Icon");
        let drawn = serde_json::json!({ "kind": "path", "path": "M0 0L1 1", "view": [0, 0, 1, 1], "alt": "Drawing",
            "stroke": { "paint": "accent", "width": "thick", "cap": "round", "join": "round" } });
        assert_eq!(kind_of(NodeType::Shape, &props(drawn)), "Drawing");
        assert_eq!(kind_of(NodeType::Stack, &props(serde_json::json!({ "fill": "surface-2" }))), "Card");
        assert_eq!(kind_of(NodeType::Group, &props(serde_json::json!({}))), "Group");
    }

    /// A shape or a container with no description of its own is named by the words it holds, at
    /// any depth: a card by its heading, a shape by the words typed in it (PLAN 2.108).
    #[test]
    fn what_holds_words_is_named_by_them() {
        let deck = Deck::from_json(
            &serde_json::json!({
                "scaena": crate::version::DECK.current, "canvas": { "width": 1920, "height": 1080 },
                "theme": "themes/dusk.theme.json",
                "nodes": {
                    "cards": { "type": "grid", "cols": 2, "at": { "col": [1, 12], "row": [3, 9] } },
                    "card": { "type": "stack", "fill": "surface-2", "at": { "parent": "cards" } },
                    "label": { "type": "text", "role": "label", "text": "Demographics", "at": { "parent": "card" } },
                    "said": { "type": "text", "role": "body", "text": "Who comes.", "at": { "parent": "card" } },
                    "start": { "type": "shape", "kind": "rect", "fill": "accent", "at": { "rect": [0, 0, 200, 100] } },
                    "start-words": { "type": "text", "role": "body", "text": "Start", "at": { "parent": "start" } },
                    "blank": { "type": "shape", "kind": "ellipse", "fill": "accent", "at": { "rect": [300, 0, 200, 100] } }
                },
                "states": [{ "id": "s", "mode": "absolute", "props": {
                    "cards": {}, "card": {}, "label": {}, "said": {}, "start": {}, "start-words": {}, "blank": {}
                } }]
            })
            .to_string(),
        )
        .unwrap();
        let snap = crate::tracking::resolve_states(&deck).unwrap().remove(0);
        let named = |id: &str| object_in(&deck, &snap.nodes, id);
        assert_eq!(named("card"), "Demographics");
        assert_eq!(named("cards"), "Demographics", "by the first words in what it holds");
        assert_eq!(named("start"), "Start");
        assert_eq!(named("blank"), "Oval");
        assert_eq!(named("said"), "Who comes.");
    }

    /// An exit is said as a presentation app names its Build Out effects: the entrance played
    /// back, `moveInLeft` as Move Out to Left and `appear` as Disappear; one that plays the same
    /// both ways, Pop, as it enters.
    #[test]
    fn an_exit_is_said_as_one() {
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        assert_eq!(leaving("moveInLeft", &theme), "Move Out to Left");
        assert_eq!(leaving("appear", &theme), "Disappear");
        assert_eq!(leaving("fade", &theme), "Fade Out");
        assert_eq!(leaving("rise", &theme), "Sink");
        assert_eq!(leaving("pop", &theme), "Pop");
        assert_eq!(leaving("fadeScale", &theme), "Fade and Scale");
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let said = lexicon(&deck, &theme, "intro").unwrap();
        assert_eq!(said.names["motion-exit"]["moveInRight"], "Move Out to Right");
        assert_eq!(said.names["motion-exit"]["appear"], "Disappear");
        assert_eq!(said.names["motion-preset"]["moveInRight"], "Move In from Right", "an entrance as it was");
    }

    /// A chart's channels are labeled by its kind: a donut's categories are its Slices, and bars
    /// that run across name theirs down the side.
    #[test]
    fn a_charts_channels_are_said_by_its_kind() {
        assert_eq!(channel("x/field", Some("donut"), false), Some("Slices"));
        assert_eq!(channel("y/field", Some("donut"), false), Some("Values"));
        assert_eq!(channel("x/field", Some("bar"), true), Some("Down the Side"));
        assert_eq!(channel("y/field", Some("bar"), true), Some("Across"));
        assert_eq!(channel("x/field", Some("bar"), false), None, "LABELS says it: Across");
        assert_eq!(channel("x/field", Some("line"), true), None, "a line does not turn");
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let mut doc: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        doc["nodes"]["rev"]["kind"] = serde_json::json!("donut");
        let deck: Deck = serde_json::from_value(doc).unwrap();
        let files = std::collections::BTreeMap::<String, Vec<u8>>::new();
        let mut c = crate::choices::choices(&deck, &theme, "revenue", "rev", &files).unwrap();
        choices(&mut c, &deck, &theme);
        let label = |prop: &str| {
            c.fields.iter().find(|f| f.prop == prop).and_then(|f| f.words.as_ref()).map(|w| w.label.clone())
        };
        assert_eq!(label("kind").as_deref(), Some("Chart"));
        assert_eq!(label("x/type").as_deref(), Some("Slices Are"));
        let exit = c.fields.iter().find(|f| f.prop == "exit").unwrap().words.as_ref().unwrap();
        assert_eq!(exit.said["moveInLeft"], "Move Out to Left", "Build Out offers exits");
        assert_eq!(c.noun.as_deref(), Some("Chart"));
    }

    /// A data source is named as a person names it (PLAN 3.18), never by the id the deck writes it
    /// with: by its file's name in words, else, its rows written in the deck, by its own.
    #[test]
    fn a_data_source_is_named_by_its_file() {
        assert_eq!(source("funding", Some("data/trails-funding.csv")), "Trails Funding");
        assert_eq!(source("q3", Some("data/q3-revenue.csv")), "Q3 Revenue");
        assert_eq!(source("segments", None), "Segments", "rows written in the deck");
        assert_eq!(source("odd", Some("data/.csv")), "Odd", "a file whose name says nothing");
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let files = std::collections::BTreeMap::<String, Vec<u8>>::new();
        let mut c = crate::choices::choices(&deck, &theme, "revenue", "rev", &files).unwrap();
        choices(&mut c, &deck, &theme);
        let data = c.fields.iter().find(|f| f.prop == "data").and_then(|f| f.words.as_ref()).unwrap();
        assert_eq!(data.said["@q3"], "Q3 Revenue", "the chart's source, by its file");
        // The words an editor names each source by, wherever it speaks of one: Insert's charts and
        // tables of it, the rows a mark is drawn from, the file it reads.
        let said = lexicon(&deck, &theme, "revenue").unwrap();
        assert_eq!(said.sources, BTreeMap::from([("q3".to_string(), "Q3 Revenue".to_string())]));
    }

    /// A polygon of the shape library is named as Insert names it (PLAN 2.106), wherever an editor
    /// names an object: a star is a Star. A polygon of its own is a Polygon, a rectangle a
    /// Rectangle, and a description comes first.
    #[test]
    fn a_shape_of_the_library_is_named_by_its_shape() {
        let props = |v: Value| -> Props { serde_json::from_value(v).unwrap() };
        for (name, points, _) in crate::inserts::polygons() {
            let drawn = props(serde_json::json!({ "kind": "polygon", "points": points, "fill": "accent" }));
            assert_eq!(object(NodeType::Shape, &drawn), shape(name), "{name}");
        }
        let own = props(serde_json::json!({ "kind": "polygon", "points": [[0, 0], [1, 0], [0.4, 1]] }));
        assert_eq!(object(NodeType::Shape, &own), "Polygon");
        assert_eq!(object(NodeType::Shape, &props(serde_json::json!({ "kind": "rect" }))), "Rectangle");
        let star = crate::inserts::polygons().into_iter().find(|(n, _, _)| *n == "star").unwrap().1;
        let described = props(serde_json::json!({ "kind": "polygon", "points": star, "alt": "A gold star" }));
        assert_eq!(object(NodeType::Shape, &described), "A gold star");
    }
}
