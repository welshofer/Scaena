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
];

/// A word said alike whatever property takes it: how a chart's channel reads its column.
pub const ANY_WORDS: &[(&str, &str)] =
    &[("quantitative", "Numbers"), ("ordinal", "In Order"), ("nominal", "Names"), ("temporal", "Dates")];

/// One of the words `prop` takes, said: from [`WORDS`] and [`ANY_WORDS`]; a data source by its
/// name, without the `@` the deck writes it with; else the word as words.
pub fn word(prop: &str, word: &str) -> String {
    if let Some((_, _, said)) = WORDS.iter().find(|(p, w, _)| *p == prop && *w == word) {
        return said.to_string();
    }
    if let Some((_, said)) = ANY_WORDS.iter().find(|(w, _)| *w == word) {
        return said.to_string();
    }
    match word.strip_prefix('@') {
        Some(source) => source.to_string(),
        None => phrase(word),
    }
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
    ("fadeScale", "Fade and Scale"),
    ("scaleBig", "Scale Big"),
    ("moveInLeft", "Move In from Left"),
    ("moveInRight", "Move In from Right"),
    ("moveInTop", "Move In from Top"),
    ("moveInBottom", "Move In from Bottom"),
];

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

/// A node's kind as `props` show it, as a person names it: a connector is an Arrow or a Line
/// (PLAN 2.109), and anything else its type's kind.
pub fn kind_of(t: NodeType, props: &Props) -> &'static str {
    match props.get("kind").and_then(Value::as_str) {
        Some("arrow") if t == NodeType::Shape && props.contains_key("connect") => "Arrow",
        Some("line") if t == NodeType::Shape && props.contains_key("connect") => "Line",
        _ => kind(Some(t)),
    }
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
    if line.chars().count() > NAMED {
        let cut: String = line.chars().take(NAMED - 1).collect();
        return format!("{}…", cut.trim_end());
    }
    line.to_string()
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

/// `state` as [`slides`] numbers it: `Slide 3, step 2`; a state the deck has not, `Slide`.
pub fn slide(deck: &Deck, state: &str) -> String {
    match deck.states.iter().position(|s| s.id == state) {
        Some(i) => slides(deck).swap_remove(i),
        None => "Slide".into(),
    }
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
    ("W231", "Its font has no italic, so it's set upright."),
    ("W232", "Its font has no bold, so it's set in its own weight."),
    ("W303", "The slide's number or footer is drawn over it."),
    ("W310", "Some of the chart's labels overlap."),
    ("W311", "A background effect is behind this chart."),
    ("W312", "Some of the chart's text is too small to read."),
    ("W313", "The chart is too small to read."),
    ("W410", "The picture has no description for people who can't see it."),
    ("W427", "A figure here no longer matches its data."),
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
];

/// What lint finds, said in a sentence, by its code: what an editor marks ([`MARKED`]), then what
/// it lists ([`LISTED`]). None for a code lint does not have.
pub fn issue(code: &str) -> Option<&'static str> {
    MARKED.iter().chain(LISTED).find(|(c, _)| *c == code).map(|(_, said)| *said)
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

/// A property's words, as an editor shows them (PLAN 2.99): its label, and each name or word it
/// offers, the value, and the theme's, said.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldWords {
    /// The property as a presentation app labels it: `Font`, `Build In`, `Advance`.
    pub label: String,
    /// Each name or word offered, and the value and the theme's where they are one, said:
    /// `onSurface` as `Text`, `fast` as `Fast (0.18 s)`, `cover` as `Fill`; a weight by its name.
    pub said: BTreeMap<String, String>,
}

/// `field`'s words, its names said as `theme` has them.
pub fn field(field: &Field, theme: &Theme) -> FieldWords {
    let say = |key: &str| match &field.takes {
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
    FieldWords { label: label(&field.prop), said }
}

/// `choices` worded: the node named by what it shows (`said`) and by its kind (`noun`), each field
/// labeled and its values said, in `theme`.
pub fn choices(choices: &mut Choices, deck: &Deck, theme: &Theme) {
    let shown = crate::tracking::resolve_states(deck)
        .ok()
        .and_then(|snaps| snaps.into_iter().find(|s| s.state_id == choices.state))
        .and_then(|snap| snap.nodes.get(&choices.node).cloned());
    choices.said = Some(match &shown {
        Some(props) => object(choices.node_type, props),
        None => kind(Some(choices.node_type)).to_string(),
    });
    // A filled path on a view is an icon (PLAN 2.104), as Insert makes one: what the picker
    // replaces. A pen's drawing, stroked, is a shape.
    let icon = choices.node_type == NodeType::Shape
        && shown.as_ref().is_some_and(|p| crate::inserts::is_icon(|key| p.get(key)));
    choices.noun = Some(if icon { "Icon" } else { kind(Some(choices.node_type)) }.to_string());
    for f in &mut choices.fields {
        f.words = Some(field(f, theme));
    }
}

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
/// state shows named by what it shows and by its kind, each of the theme's names said, and how a
/// motion, its split, a shape, and each issue lint finds are said.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lexicon {
    /// Each node, by its id: what it shows, as the state shows it where it does, else its kind
    /// ([`object`]).
    pub objects: BTreeMap<String, String>,
    /// Each node, by its id: its kind ([`kind`]).
    pub kinds: BTreeMap<String, String>,
    /// Each of the theme's names said ([`named`]), by its kind of name: `color`, `text-role`,
    /// `layout`, `motion-preset`, …
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
        objects.insert(id.clone(), object(node.node_type, props));
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
    for split in ["lines", "words", "glyphs", "children", "marks"] {
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
    Ok(Lexicon { objects, kinds, names, motions, splits, shapes, places, weights, types, issues })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(word("data", "@q3"), "q3");
        assert_eq!(word("semantic", "takeaway"), "Takeaway");
        assert_eq!(motion("enter", Some("words")), "Enters Word by Word");
        assert_eq!(motion("exit", None), "Leaves");
        assert_eq!(weight(700.0), "Bold");
        assert_eq!(weight(650.0), "650");
        assert_eq!(theme("themes/dusk.theme.json"), "Dusk");
        assert_eq!(theme("themes/dusk-2.theme.json"), "Dusk 2");
    }
}
