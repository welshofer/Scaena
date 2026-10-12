//! A deck made from a narrative (ADR-0026, PLAN G.1). The spine comes first: each run of slides
//! in one beat of the arc is a section, and each slide a beat, its title the claim, its speaker
//! notes the notes, its length Narrato's estimate, a chart's data its evidence, and the rest of
//! Narrato's fields in `media.narrato`. Then each beat's state: a slide in the theme layout its
//! Narrato layout comes to, chosen by how many items it has where a layout comes in sizes, after
//! Narrato's variety pass. Each slot is filled from the beat's fields: words in the slot's role,
//! a list where it waits for one, a picture slot the theme's shader art with the visual concept
//! as its description, a chart's figures a data source of its own. A slot with nothing to say is
//! left out, and the card under it.

use crate::create::{Create, Created, creating};
use crate::lint::{View, Write, lint_in};
use crate::{Context, OpsError};
use scaena_core::inserts::slug;
use scaena_core::model::theme::{Example, Prompt, PromptItems, Slot, Theme};
use scaena_core::{Deck, Finding};
use scaena_narrato::layouts::{self, Varied};
use scaena_narrato::{ChartData, Narrative, Slide};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// The canvas a made deck draws on.
const CANVAS: [u32; 2] = [1920, 1080];

/// What a narrative made: the deck, as `deck.json` holds it, less its theme and fonts, which
/// [`creating`] sets; and the layout each slide took.
#[derive(Debug, Clone, PartialEq)]
pub struct Made {
    pub deck: Value,
    pub layouts: Vec<String>,
}

/// A deck generated: the bundle made, as `deck_create` reports one, each slide's layout, and
/// the findings whose fixes were applied, each checked by laying its state out again.
#[derive(Debug, Clone)]
pub struct Generated {
    pub created: Created,
    pub layouts: Vec<String>,
    pub fixed: Vec<Finding>,
}

/// The bundle `narrative` makes in the theme whose file is `theme_file` and whose JSON is
/// `theme`, each font it names read by `font`: [`deck`] made into a bundle as
/// [`creating`] makes one, then each fix lint offers applied, as `lint --fix` applies them, so a
/// text too long for its slot shrinks to fit. Nothing is written: the deck and every file beside
/// it, if it validates.
pub fn generating(
    narrative: &Narrative,
    theme_file: &Path,
    theme: String,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
) -> Result<(Generated, Option<Write>), OpsError> {
    let model: Theme = serde_json::from_str(&theme).context("the theme")?;
    let made = deck(narrative, &model)?;
    let req = Create { theme: PathBuf::from(theme_file), deck: Some(made.deck), ..Create::default() };
    let none = |path: &Path| -> Result<Vec<u8>, OpsError> {
        Err(OpsError::new(format!("a generated deck reads no file: {}", path.display())))
    };
    let (mut created, write) = creating(&req, theme, font, &none)?;
    let Some(mut write) = write else {
        return Ok((Generated { created, layouts: made.layouts, fixed: Vec::new() }, None));
    };
    let nothing = scaena_store::Files::Zip(Default::default());
    let view = View { base: &nothing, pending: write.files.clone() };
    let mut doc = write.deck.to_value()?;
    let mut fixed = Vec::new();
    // Characters the theme's fonts have no glyph for, as plain text writes them: CO₂ as CO2.
    let unset: Vec<Finding> = created.findings.iter().filter(|f| f.code == "E120").cloned().collect();
    if !unset.is_empty() {
        for f in &unset {
            plainer(&mut doc, f);
        }
        fixed.extend(unset);
        let deck = Deck::from_json(&doc.to_string()).context("the deck in plain characters")?;
        created.findings = lint_in(&deck, &view)?.findings;
    }
    // Then each fix lint offers, each checked by laying its state out again with it.
    let offered: Vec<Finding> = created.findings.iter().filter(|f| f.fix.is_some()).cloned().collect();
    for f in offered {
        let mut next = doc.clone();
        if scaena_core::patch::apply(&mut next, f.fix.as_deref().unwrap_or_default()).is_ok() {
            doc = next;
            fixed.push(f);
        }
    }
    if !fixed.is_empty() {
        let deck = Deck::from_json(&doc.to_string()).context("the fixed deck")?;
        created.findings = lint_in(&deck, &view)?.findings;
        created.errors = created.findings.iter().filter(|f| f.severity == scaena_core::Severity::Error).count();
        write.deck = deck;
    }
    Ok((Generated { created, layouts: made.layouts, fixed }, Some(write)))
}

/// `doc` with the characters an E120 finding names written as plain text writes them, where the
/// node draws them: its text, a chart's titles, and the cells of the data a chart reads.
fn plainer(doc: &mut Value, finding: &Finding) {
    let Some(node) = finding.node.as_deref() else { return };
    let missing: Vec<char> = (finding.measure.as_ref())
        .and_then(|m| m["missing"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str()?.chars().next())
        .collect();
    let swap = |text: &str| -> String {
        text.chars().map(|c| if missing.contains(&c) { plain(c).to_string() } else { c.to_string() }).collect()
    };
    let Some(n) = doc.pointer_mut(&format!("/nodes/{node}")) else { return };
    if let Some(text) = n["text"].as_str() {
        n["text"] = json!(swap(text));
    }
    for axis in ["x", "y"] {
        if let Some(title) = n[axis]["title"].as_str() {
            n[axis]["title"] = json!(swap(title));
        }
    }
    let Some(source) = n["data"].as_str().and_then(|d| d.strip_prefix('@')).map(str::to_string) else { return };
    let rows = doc.pointer_mut(&format!("/data/{source}/source/inline")).and_then(Value::as_array_mut);
    for cell in rows.into_iter().flatten().filter_map(Value::as_object_mut).flat_map(|r| r.values_mut()) {
        if let Some(text) = cell.as_str() {
            *cell = json!(swap(text));
        }
    }
}

/// A character as plain text writes it: a subscript or a superscript digit as the digit, a
/// fraction with a slash, a mark in letters, punctuation as ASCII has it; nothing for the rest,
/// an emoji or a symbol no font of the theme draws.
fn plain(c: char) -> &'static str {
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    match c {
        '\u{2080}'..='\u{2089}' => DIGITS[c as usize - 0x2080],
        '\u{2070}' => "0",
        '\u{b9}' => "1",
        '\u{b2}' => "2",
        '\u{b3}' => "3",
        '\u{2074}'..='\u{2079}' => DIGITS[c as usize - 0x2070],
        '\u{208a}' | '\u{207a}' => "+",
        '\u{208b}' | '\u{207b}' | '\u{2212}' | '\u{2013}' | '\u{2014}' | '\u{2010}' | '\u{2011}' => "-",
        '\u{bd}' => "1/2",
        '\u{bc}' => "1/4",
        '\u{be}' => "3/4",
        '\u{2153}' => "1/3",
        '\u{2154}' => "2/3",
        '\u{2122}' => "TM",
        '\u{ae}' => "(R)",
        '\u{a9}' => "(C)",
        '\u{2026}' => "...",
        '\u{2018}' | '\u{2019}' | '\u{2032}' => "'",
        '\u{201c}' | '\u{201d}' | '\u{2033}' => "\"",
        '\u{2022}' | '\u{b7}' => "-",
        '\u{2192}' => "->",
        '\u{2190}' => "<-",
        '\u{2265}' => ">=",
        '\u{2264}' => "<=",
        '\u{2248}' => "~",
        '\u{d7}' => "x",
        '\u{a0}' | '\u{2009}' | '\u{202f}' => " ",
        _ => "",
    }
}

/// The deck `narrative` makes in `theme`.
pub fn deck(narrative: &Narrative, theme: &Theme) -> Result<Made, OpsError> {
    if narrative.slides.is_empty() {
        return Err(OpsError::new("a narrative with no slides makes no deck"));
    }
    let arc = narrative.narrative_arc();
    let names = arc.beat_names();
    let count = narrative.slides.len();
    let mut varied: Vec<Varied> = narrative.slides.iter().map(Varied::of).collect();
    layouts::vary(&mut varied, true);

    let mut deck = Builder::default();
    let mut sections: Vec<Value> = Vec::new();
    let mut last_beat = None;
    let mut chosen = Vec::with_capacity(count);
    let mut section_number = 0;
    for (i, (slide, v)) in narrative.slides.iter().zip(&varied).enumerate() {
        let beat = (slide.beat_index.filter(|b| *b < names.len()))
            .unwrap_or_else(|| scaena_narrato::beats::beat_of(i, count, names.len()));
        let items = items(slide, v);
        let layout = layout_for(v.layout.identity(), slide, &items, i, theme);
        if layout == "section" {
            section_number += 1;
        }
        let state = deck.state_id(&slide.slide_title, &layout);
        let slots = Slots {
            theme,
            layout: &layout,
            state: &state,
            slide,
            body: &v.body,
            items: &items,
            section: section_number,
        };
        let made = deck.slide(slots, i > 0)?;
        chosen.push(layout);

        // The spine: a section for each run of slides in one beat, a beat for each slide.
        if last_beat != Some(beat) {
            let name = names.get(beat).copied().unwrap_or("Section");
            let id = deck.unique(&slug(name, "section"), Taken::Section);
            sections.push(json!({ "id": id, "title": name, "beats": [] }));
            last_beat = Some(beat);
        }
        let claim = if slide.slide_title.is_empty() { format!("Slide {}", i + 1) } else { slide.slide_title.clone() };
        let beat_id = deck.unique(&slug(&claim, "beat"), Taken::Beat);
        let seconds = scaena_narrato::duration::seconds(
            &slide.speaker_notes,
            &v.body,
            &slide.slide_title,
            v.layout.layout,
            &slide.visual_description,
        );
        let mut entry = json!({ "id": beat_id, "claim": claim, "states": [state], "duration": seconds });
        if !slide.speaker_notes.is_empty() {
            entry["notes"] = json!(slide.speaker_notes);
        }
        if let Some(data) = &made.data {
            entry["evidence"] = json!([format!("@{data}")]);
        }
        entry["media"] = json!({ "narrato": media(slide) });
        sections.last_mut().expect("opened above")["beats"].as_array_mut().expect("an array").push(entry);
    }

    let brief = &narrative.brief;
    let title = narrative.slides.first().map(|s| s.slide_title.clone()).filter(|t| !t.is_empty());
    let mut meta = json!({
        "title": title.unwrap_or_else(|| brief.topic.clone()),
        "narrative": {
            "generator": "narrato",
            "arc": narrative.arc,
            "reasoning": narrative.arc_reasoning,
            "brief": {
                "topic": brief.topic,
                "audience": brief.audience,
                "goal": brief.goal,
                "length": brief.length,
            },
        },
    });
    if !narrative.executive_summary.is_empty() {
        meta["description"] = json!(narrative.executive_summary);
    }
    if let Some(lang) = brief.language.as_deref().and_then(language_tag) {
        meta["lang"] = json!(lang);
    }
    if narrative.truncated {
        meta["narrative"]["truncated"] = json!(true);
    }
    let mut out = json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "meta": meta,
        "canvas": { "width": CANVAS[0], "height": CANVAS[1] },
        "spine": { "sections": sections },
        "nodes": Value::Object(deck.nodes),
        "states": deck.states,
    });
    if !deck.data.is_empty() {
        out["data"] = Value::Object(deck.data);
    }
    Ok(Made { deck: out, layouts: chosen })
}

/// Narrato's fields of a slide that the deck keeps beside its beat.
fn media(slide: &Slide) -> Value {
    let mut m = Map::new();
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v {
            m.insert(k.into(), v);
        }
    };
    put("layout", Some(json!(slide.layout_suggestion)));
    put("beatIndex", slide.beat_index.map(|v| json!(v)));
    put("beatName", slide.beat_name.as_ref().map(|v| json!(v)));
    put("purpose", slide.purpose.as_ref().map(|v| json!(v)));
    put("intent", slide.intent.as_ref().map(|v| json!(v)));
    put("intensity", slide.expected_intensity.map(|v| json!(v)));
    put("valence", slide.expected_valence.map(|v| json!(v)));
    put("visual", Some(&slide.visual_description).filter(|v| !v.is_empty()).map(|v| json!(v)));
    put("fact", slide.supporting_facts.as_ref().filter(|v| !v.is_empty()).map(|v| json!(v)));
    Value::Object(m)
}

/// The BCP 47 tag of a language the brief names by its English name.
fn language_tag(language: &str) -> Option<&'static str> {
    Some(match language {
        "Spanish" => "es",
        "French" => "fr",
        "German" => "de",
        "Portuguese" => "pt",
        "Italian" => "it",
        "Dutch" => "nl",
        "Czech" => "cs",
        "Polish" => "pl",
        "Ukrainian" => "uk",
        "Japanese" => "ja",
        "Chinese (Simplified)" => "zh-Hans",
        "Korean" => "ko",
        "Arabic" => "ar",
        "Hindi" => "hi",
        _ => return None,
    })
}

/// One of a slide's items: a bullet, and its label and detail where it reads "label: detail".
#[derive(Debug, Clone, PartialEq)]
struct Item {
    whole: String,
    label: Option<String>,
    detail: String,
}

impl Item {
    /// Whether it leads with its figure: "$4.1M: Q3 revenue, up 12%".
    fn figure_first(&self) -> bool {
        self.label.as_deref().is_some_and(numeric)
    }

    /// Its figure: the label that is one, else the first in its detail.
    fn figure(&self) -> Option<String> {
        match &self.label {
            Some(label) if numeric(label) => Some(label.clone()),
            _ => figure(&self.detail),
        }
    }

    /// What it names: its label, or, where it leads with its figure, its detail's first clause.
    fn name(&self) -> Option<String> {
        if self.figure_first() { Some(head(&self.detail).0) } else { self.label.clone() }
    }
}

/// A slide's items: its bullets, each without the mark a model may lead it with.
fn items(slide: &Slide, v: &Varied) -> Vec<Item> {
    // A cover and a divider say their line alone; a Q&A, set as a divider, keeps its questions.
    if matches!(v.layout.identity(), "title" | "section") {
        return Vec::new();
    }
    slide.key_bullets.iter().map(|b| item(b)).filter(|i| !i.whole.is_empty()).collect()
}

fn item(bullet: &str) -> Item {
    let whole = bullet.trim().trim_start_matches(['\u{2022}', '-', '*', '\u{2013}']).trim().to_string();
    // A label is short, and ends at the first ": ", " — ", or " – ".
    let split = [": ", " \u{2014} ", " \u{2013} "]
        .iter()
        .filter_map(|sep| whole.find(sep).map(|at| (at, sep.len())))
        .min_by_key(|(at, _)| *at)
        .filter(|(at, _)| (1..=60).contains(&whole[..*at].chars().count()));
    match split {
        Some((at, len)) => {
            Item { label: Some(whole[..at].trim().to_string()), detail: whole[at + len..].trim().to_string(), whole }
        }
        None => Item { label: None, detail: whole.clone(), whole },
    }
}

/// `text`'s first clause and the rest: "Q3 revenue, up 12%" → ("Q3 revenue", "up 12%").
fn head(text: &str) -> (String, String) {
    let at = [", ", "; ", " \u{2014} ", " \u{2013} "]
        .iter()
        .filter_map(|sep| text.find(sep).map(|at| (at, sep.len())))
        .min_by_key(|(at, _)| *at);
    match at {
        Some((at, len)) => (text[..at].trim().to_string(), text[at + len..].trim().to_string()),
        None => (text.trim().to_string(), String::new()),
    }
}

/// The first figure in `text`: a number with its currency sign, its suffix, or its percent.
fn figure(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.iter().position(|c| c.is_ascii_digit())?;
    let mut begin = start;
    while begin > 0 && matches!(chars[begin - 1], '$' | '\u{20ac}' | '\u{a3}' | '\u{a5}' | '+' | '-' | '\u{2212}' | '~')
    {
        begin -= 1;
    }
    let mut end = start;
    while end < chars.len() && (chars[end].is_ascii_digit() || matches!(chars[end], ',' | '.')) {
        end += 1;
    }
    while end > start && matches!(chars[end - 1], ',' | '.') {
        end -= 1;
    }
    let rest: String = chars[end..].iter().take(3).collect();
    let suffix = ["%", "\u{d7}", "x", "M", "B", "K", "k", "bn", "m", "pp", "pts"]
        .iter()
        .filter(|s| rest.starts_with(*s))
        .max_by_key(|s| s.len())
        .map_or(0, |s| s.chars().count());
    // A suffix is the figure's only where the word stops there: "5 M" no, "5M" yes, "5Mbps" no.
    let after = chars.get(end + suffix).copied();
    let suffix = if suffix > 0 && after.is_none_or(|c| !c.is_alphanumeric()) { suffix } else { 0 };
    Some(chars[begin..end + suffix].iter().collect())
}

/// Whether `text` is a figure and little else, what a number's place shows: "42%", "$4.1M",
/// "2024", "6 weeks", "212 kt".
fn numeric(text: &str) -> bool {
    let text = text.trim();
    let Some(rest) = figure(text).and_then(|f| text.strip_prefix(f.as_str()).map(str::trim)) else { return false };
    let n = rest.chars().count();
    n <= 2 || (n <= 10 && rest.chars().all(char::is_alphabetic))
}

/// The theme layout a slide is drawn in: what its identity comes to in Scaena's library, sized
/// to its items, else a list of its bullets, else the first the theme has.
fn layout_for(identity: &str, slide: &Slide, items: &[Item], index: usize, theme: &Theme) -> String {
    let n = items.len();
    let chart = slide.chart_data.is_some();
    let words = format!("{} {}", slide.slide_title, slide.subtitle).to_lowercase();
    let labelled = n > 0 && items.iter().all(|i| i.label.is_some());
    let wanted: &str = match identity {
        "title" => "title",
        "section" => "section",
        "agenda" => "agenda",
        "closing" | "thanks" => "closing",
        "body" => "bullets",
        "numbered" => "numbered",
        "quote" => "quote",
        "statement" => "statement",
        "definition" => "definition",
        "metric" => "big-number",
        "scorecard" | "kpi-grid" => match n {
            0..=2 => "two-numbers",
            3 => "numbers",
            4 => "scorecard",
            _ => "number-grid",
        },
        "bar-chart" if chart => "bar-chart",
        "line-chart" if chart => "line-chart",
        "bar-chart" | "line-chart" => "bullets",
        "two-column" if n == 3 && labelled => "three-columns",
        "two-column" if n == 4 && labelled => "quadrants",
        "two-column" if labelled => "comparison",
        "two-column" => "two-columns",
        "before-after" => "before-after",
        "pros-cons" => "pros-cons",
        "quadrant" => "quadrants",
        "steps" if n <= 3 => "how-it-works",
        "steps" => "steps",
        "timeline" => "timeline",
        "journey" => "journey",
        "image" => "full",
        "image-text" => {
            if index.is_multiple_of(2) {
                "art-left"
            } else {
                "art-right"
            }
        }
        "gallery" => match n {
            0..=2 => "two-up",
            3 => "three-up",
            4 => "four-up",
            _ => "six-up",
        },
        "framework" => {
            if ["cycle", "loop", "flywheel", "circular"].iter().any(|w| words.contains(w)) {
                "cycle"
            } else if ["venn", "overlap", "intersect"].iter().any(|w| words.contains(w)) || n == 3 {
                "venn"
            } else if ["hub", "spoke", "core", "center", "centre"].iter().any(|w| words.contains(w)) {
                "hub-spoke"
            } else {
                "pyramid"
            }
        }
        "callout" => "callout",
        "roadmap" => "roadmap",
        "matrix" => "matrix",
        "okrs" => "okrs",
        "raci" => "raci",
        "swot" => "swot",
        "thesis" => "thesis",
        "letter" => "letter",
        "memo" => "memo",
        "receipt" => "receipt",
        "next-steps" => "next-steps",
        "milestones" => "milestones",
        "3horizons" => "three-horizons",
        "funnel" => "funnel",
        "scenarios" => "scenarios",
        "vs" => "head-to-head",
        "ranking" => "ranking",
        "headline" => "headline",
        "manifesto" => "manifesto",
        "team" => match n {
            0..=3 => "team",
            4 => "team-four",
            _ => "team-six",
        },
        "bio" => "bio",
        "testimonials" if n <= 2 => "two-quotes",
        "testimonials" => "testimonials",
        "how-it-works" => "how-it-works",
        "pipeline" => "pipeline",
        "stack" => "stack",
        "faq" => "faq",
        "glossary" => "glossary",
        "checklist" => "checklist",
        "ask" => "the-ask",
        "qa" => "qa",
        "recipe-card" => "recipe",
        _ => "bullets",
    };
    // A layout with no place for some of the items gives way to a list of them all.
    let sequential = [
        "steps", "timeline", "milestones", "how-it-works", "pipeline", "funnel", "roadmap", "agenda", "numbered",
        "ranking", "manifesto",
    ];
    let wanted = match holds(theme, wanted, n) {
        true => wanted,
        false if sequential.contains(&identity) => "numbered",
        false => "bullets",
    };
    // A theme with none of the library's names draws it in its first layout.
    [wanted, "bullets", "statement"]
        .into_iter()
        .find(|l| theme.layouts.contains_key(*l))
        .or_else(|| theme.layouts.keys().next().map(String::as_str))
        .unwrap_or("bullets")
        .to_string()
}

/// Whether `layout` has a place for each of `n` items: a list or a body holds any number; a
/// layout of places, one item for each ordinal; a line with its support, one; a quote, a number,
/// or a definition, the two it sets beside it. A chart takes its takeaway alone, its figures in
/// its data.
fn holds(theme: &Theme, layout: &str, n: usize) -> bool {
    let Some(l) = theme.layouts.get(layout) else { return true };
    match layout {
        // A pros-and-cons slide sorts its items onto two lists.
        "title" | "section" | "bar-chart" | "line-chart" | "pros-cons" => return true,
        "statement" | "closing" | "thesis" | "headline" => return n <= 1,
        "quote" | "big-number" | "definition" => return n <= 2,
        _ => {}
    }
    let lists = l.slots.iter().any(|(name, s)| {
        ordinal(name).is_none()
            && (matches!(s.prompt, Some(Prompt::Items(_))) || matches!(name.as_str(), "body" | "story" | "details"))
    });
    let places = l.slots.keys().filter_map(|name| ordinal(name).map(|(k, _)| k + 1)).max().unwrap_or(0);
    // A head-to-head's first item names the two sides; a receipt's last is its total.
    let extra = usize::from(matches!(layout, "head-to-head" | "receipt"));
    lists || places == 0 || n <= places + extra
}

/// `text` as a sentence begins: its first word capitalized where it is all lower case, as the
/// detail after a label begins ("cuts emissions 80%"); a word that has a capital of its own
/// ("iPhone") left as it is.
fn sentence(text: &str) -> String {
    let word = text.split_whitespace().next().unwrap_or_default();
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_lowercase() && word.chars().all(|c| !c.is_uppercase()) => {
            first.to_uppercase().chain(chars).collect()
        }
        _ => text.to_string(),
    }
}

/// What a slide's slots are filled from.
struct Slots<'a> {
    theme: &'a Theme,
    layout: &'a str,
    state: &'a str,
    slide: &'a Slide,
    /// Its body after the variety pass: a bullet a cover or a closing took for its line.
    body: &'a str,
    items: &'a [Item],
    /// Which section divider this is, from 1.
    section: usize,
}

/// What one slide added beyond its nodes: the data source its chart reads.
struct SlideMade {
    data: Option<String>,
}

/// What fills a slot.
enum Fill {
    Text(String),
    List(Vec<String>, &'static str),
    /// The prompt's own words: a column's or a lane's label.
    Prompt,
    Shape,
    Picture,
    Chart,
}

const ORDINALS: [&str; 9] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth"];

/// A slot's ordinal and what part of the item it holds: `second-title` → (1, "title"),
/// `third` → (2, "").
fn ordinal(slot: &str) -> Option<(usize, &str)> {
    ORDINALS.iter().enumerate().find_map(|(k, o)| {
        if slot == *o {
            Some((k, ""))
        } else {
            slot.strip_prefix(o).and_then(|r| r.strip_prefix('-')).map(|part| (k, part))
        }
    })
}

/// Slots whose prompt names a structure the slide keeps: a column's or a lane's label, a rank, a
/// step's numeral, an axis.
fn keeps_prompt(slot: &str) -> bool {
    slot.ends_with("-label")
        || slot.ends_with("-lane")
        || slot.ends_with("-role")
        || slot.ends_with("-rank")
        || slot.ends_with("-number")
        || slot.starts_with("month-")
        || matches!(slot, "needs-title" | "method-title" | "x-low" | "x-high" | "y-low" | "y-high")
}

/// Roles that set a figure.
fn figure_role(role: &str) -> bool {
    matches!(role, "figure" | "hero" | "value" | "numeral")
}

fn unquoted(text: &str) -> String {
    text.trim().trim_matches(['"', '\u{201c}', '\u{201d}', '\u{2018}', '\u{2019}', '\'']).trim().to_string()
}

/// "words — who": the words, and who said them.
fn said(item: &Item) -> (String, Option<String>) {
    match item.whole.rsplit_once(" \u{2014} ").or_else(|| item.whole.rsplit_once(" \u{2013} ")) {
        Some((words, who)) => (unquoted(words), Some(who.trim().to_string())),
        None => (unquoted(&item.whole), None),
    }
}

/// An item cut into its parts at " — " or " | ".
fn parts(text: &str) -> Vec<String> {
    let sep = if text.contains(" \u{2014} ") { " \u{2014} " } else { " | " };
    text.split(sep).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// A detail cut at "; " into a list's items.
fn listed(text: &str) -> Vec<String> {
    text.split("; ").map(|p| p.trim().trim_end_matches('.').to_string()).filter(|p| !p.is_empty()).collect()
}

/// What `text` says after the figure it opens with, where the figure is a clause of its own:
/// "1,200, 28% go on" → "28% go on"; none where it opens otherwise, or the figure runs on into
/// its sentence ("$40M revenue by 2028").
fn after_figure(text: &str) -> Option<String> {
    let f = figure(text)?;
    let rest = text.trim().strip_prefix(f.as_str())?;
    if rest.is_empty() {
        return Some(String::new());
    }
    [", ", "; ", " \u{2014} ", " \u{2013} "].iter().find_map(|sep| rest.strip_prefix(sep)).map(|r| r.trim().to_string())
}


impl Slots<'_> {
    fn title(&self) -> Option<String> {
        Some(self.slide.slide_title.clone()).filter(|t| !t.is_empty())
    }

    /// The subtitle, else the line the variety pass gave a cover or a closing.
    fn subtitle(&self) -> Option<String> {
        Some(self.slide.subtitle.clone())
            .filter(|t| !t.is_empty())
            .or_else(|| Some(self.body.to_string()).filter(|b| !b.is_empty() && !b.starts_with('\u{2022}')))
    }

    fn fact(&self) -> Option<String> {
        self.slide.supporting_facts.clone().filter(|t| !t.is_empty())
    }

    fn item(&self, k: usize) -> Option<&Item> {
        self.items.get(k)
    }

    fn wholes(&self) -> Vec<String> {
        self.items.iter().map(|i| i.whole.clone()).collect()
    }

    /// Words a card holds: each item whole where every item is short, else what each names, so
    /// the cards of one slide read alike.
    fn short(&self, item: &Item) -> String {
        let long = self.items.iter().any(|i| i.label.is_some() && i.whole.chars().count() > 36);
        match &item.label {
            Some(label) if long => label.clone(),
            _ => item.whole.clone(),
        }
    }

    /// Whether the layout has a slot of this name.
    fn has(&self, slot: &str) -> bool {
        self.template().is_some_and(|l| l.slots.contains_key(slot))
    }

    fn template(&self) -> Option<&scaena_core::model::theme::Layout> {
        self.theme.layouts.get(self.layout)
    }

    /// The role of the layout's slot of this name.
    fn role_of(&self, slot: &str) -> Option<&str> {
        self.template().and_then(|l| l.slots.get(slot)).and_then(|s| s.role.as_deref())
    }

    /// What the slot holds, if anything.
    fn fill(&self, name: &str, slot: &Slot) -> Option<Fill> {
        let list_kind = match &slot.prompt {
            Some(Prompt::Items(PromptItems::Number(_))) => Some("number"),
            Some(Prompt::Items(PromptItems::Bullet(_))) => Some("bullet"),
            _ => None,
        };
        let text = |t: Option<String>| t.map(Fill::Text);
        let picture = matches!(slot.example, Some(Example::Picture(_)));
        if let Some(Prompt::Shape(_)) = &slot.prompt {
            // A card, a rule, or an arrow: with its ordinal's item, or the slide's own.
            return match ordinal(name) {
                Some((k, _)) => self.item(k).map(|_| Fill::Shape),
                None => Some(Fill::Shape),
            };
        }
        if keeps_prompt(name) {
            return match ordinal(name) {
                Some((k, _)) => self.item(k).map(|_| Fill::Prompt),
                None => Some(Fill::Prompt),
            };
        }
        if picture {
            return match ordinal(name) {
                Some((k, _)) => self.item(k).map(|_| Fill::Picture),
                None => Some(Fill::Picture),
            };
        }
        if matches!(slot.example, Some(Example::Chart(_)) | Some(Example::Table(_))) {
            return self.slide.chart_data.as_ref().map(|_| Fill::Chart);
        }
        let layout = self.layout;
        // Layouts whose places read their items in their own ways.
        if let Some(fill) = self.special(layout, name, list_kind) {
            return fill;
        }
        if let Some((k, part)) = ordinal(name) {
            let item = self.item(k)?;
            if part.is_empty()
                && let Some(kind) = list_kind
            {
                return Some(Fill::List(listed(&item.detail).iter().map(|p| sentence(p)).collect(), kind));
            }
            let role = slot.role.as_deref().unwrap_or_default();
            return text(self.part(k, item, part, role));
        }
        text(match name {
            "header" | "title" | "statement" | "claim" | "ask" | "question" | "term" | "headline" | "point"
            | "name" | "thanks" | "top" => self.title(),
            "subtitle" | "support" | "dek" | "follow" | "talk" | "lede" | "role" => {
                self.subtitle().or_else(|| self.item(0).map(|i| i.whole.clone()))
            }
            "hub" => self.subtitle().or_else(|| self.title()),
            "number" if layout == "section" => Some(format!("{:02}", self.section)),
            "number" => self.item(0).map(|i| i.figure().unwrap_or_else(|| i.whole.clone())),
            "quote" => self.item(0).map(|i| unquoted(&i.whole)).or_else(|| self.title()),
            "who" => self.item(1).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "meaning" => self.item(0).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "example" => self.item(1).map(|i| i.whole.clone()),
            "takeaway" => self.item(0).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "callout" => {
                self.fact().as_deref().and_then(figure).or_else(|| self.item(0).and_then(|i| figure(&i.whole)))
            }
            "under" | "detail" => self.fact().or_else(|| self.item(1).map(|i| i.whole.clone())),
            "note" => {
                let source =
                    self.slide.chart_data.as_ref().and_then(|c| c.source_id.clone()).map(|s| format!("Source: {s}"));
                let fallback = if layout == "full" { self.subtitle() } else { None };
                source.or_else(|| self.fact()).or(fallback)
            }
            "story" | "details" => (!self.items.is_empty()).then(|| self.wholes().join("\n")),
            "body" | "needs" | "method" => {
                if self.items.is_empty() {
                    self.subtitle()
                } else {
                    return Some(Fill::List(self.wholes(), list_kind.unwrap_or("bullet")));
                }
            }
            _ => None,
        })
    }

    /// What `part` of the `k`th item shows, in a slot of `role`: where the layout sets its figure
    /// apart, the figure; where it sets its name apart, the name; and in the slot of its own, what
    /// the others leave.
    fn part(&self, k: usize, item: &Item, part: &str, role: &str) -> Option<String> {
        let ord = ORDINALS[k];
        let sibling = |p: &str| if p.is_empty() { self.has(ord) } else { self.has(&format!("{ord}-{p}")) };
        let title_role = self.role_of(&format!("{ord}-title")).unwrap_or_default();
        // Where the layout shows the item's figure apart, and where its name.
        let figure_apart = sibling("figure") || sibling("target") || (sibling("title") && figure_role(title_role));
        let name_apart = (sibling("title") && !figure_role(title_role)) || sibling("date") || sibling("item");
        // A date sets a timeline's figure; elsewhere a figure's place takes the figure.
        let dated = matches!(self.layout, "timeline" | "milestones");
        match part {
            "title" if figure_role(role) && dated => item.label.clone().or_else(|| figure(&item.whole)),
            "title" if figure_role(role) => item.figure().or_else(|| item.label.clone()),
            "title" if item.figure_first() && !sibling("") => Some(sentence(&item.detail)),
            "title" | "item" | "task" if item.figure_first() => item.name().map(|n| sentence(&n)),
            "title" | "item" | "task" => item.name(),
            "date" => item.label.clone(),
            "figure" | "target" | "price" | "amount" => {
                item.figure().or_else(|| item.label.clone().filter(|_| part != "figure"))
            }
            "note" => said(item).1,
            "" => Some(match (&item.label, figure_apart, name_apart) {
                // "$4.1M: Q3 revenue, up 12%" with its figure and its name apart: "Up 12%".
                (Some(_), true, true) if item.figure_first() => sentence(&head(&item.detail).1),
                (Some(_), true, _) if item.figure_first() => sentence(&item.detail),
                // "Leads: 1,200, 28% go on" with its figure apart: "28% go on".
                (Some(_), true, true) => sentence(&after_figure(&item.detail).unwrap_or_else(|| item.detail.clone())),
                (Some(_), _, true) => sentence(&item.detail),
                // Its figure apart, its name here: "Houston: 41% of volume" → "Houston";
                // "Revenue: $4.1M, up 12%" → "Revenue, up 12%".
                (Some(label), true, false) => match after_figure(&item.detail) {
                    Some(rest) if !rest.is_empty() => format!("{label}, {rest}"),
                    _ => label.clone(),
                },
                _ if figure_role(role) => item.figure().unwrap_or_else(|| item.whole.clone()),
                _ if matches!(role, "title" | "headline" | "display") => self.short(item),
                _ => item.whole.clone(),
            })
            .filter(|t| !t.is_empty()),
            _ => None,
        }
    }

    /// The slots of layouts that read their items in their own ways; none where the general
    /// rules serve.
    fn special(&self, layout: &str, name: &str, list_kind: Option<&'static str>) -> Option<Option<Fill>> {
        let text = |t: Option<String>| Some(t.map(Fill::Text));
        let ord = ordinal(name);
        match (layout, name, ord) {
            // What the number means, and its source.
            ("big-number", "claim", _) => text(self.item(1).map(|i| i.whole.clone()).or_else(|| self.title())),
            ("big-number", "under", _) => text(self.fact()),
            // The objective is the header; the slide's title stands over it.
            ("okrs", "header", _) => text(self.subtitle().or_else(|| self.title())),
            ("okrs", "kicker", _) => text(self.subtitle().and(self.title())),
            ("qa", "support", _) => {
                text(self.subtitle().or_else(|| (!self.items.is_empty()).then(|| self.wholes().join("\n"))))
            }
            // "Option A: Option B", then "criterion: A's answer | B's answer".
            ("head-to-head", "first-title", _) => text(self.item(0).and_then(|i| i.label.clone())),
            ("head-to-head", "second-title", _) => text(self.item(0).map(|i| i.detail.clone())),
            ("head-to-head", _, Some((k, part))) => {
                let item = self.item(k + 1)?;
                let sides = parts(&item.detail.replace(" vs. ", " | ").replace(" vs ", " | "));
                match part {
                    "criterion" => text(item.label.clone()),
                    "left" => text(sides.first().map(|s| sentence(s))),
                    "right" => text(sides.get(1).map(|s| sentence(s))),
                    _ => None,
                }
            }
            // "Strengths: a; b" — each a quadrant's list; a roadmap's phase likewise.
            ("swot" | "roadmap", _, Some((k, ""))) => {
                let item = self.item(k)?;
                let points = listed(&item.detail).iter().map(|p| sentence(p)).collect();
                Some(Some(Fill::List(points, list_kind.unwrap_or("bullet"))))
            }
            // "Pro: …" on one side, "Con: …" on the other.
            ("pros-cons", "first-title", _) => text(Some("Pros".into())),
            ("pros-cons", "second-title", _) => text(Some("Cons".into())),
            ("pros-cons", "first" | "second", _) => {
                let (pros, cons) = self.sides(&["pro", "for", "upside", "benefit", "gain"]);
                let side = if name == "first" { pros } else { cons };
                Some((!side.is_empty()).then(|| Fill::List(side, list_kind.unwrap_or("bullet"))))
            }
            // "action — owner — when".
            ("next-steps", _, Some((k, part))) => {
                let item = self.item(k)?;
                let p = parts(&item.whole);
                text(match part {
                    "" => p.first().cloned(),
                    "owner" => p.get(1).cloned(),
                    "when" => p.get(2).cloned(),
                    _ => None,
                })
            }
            // "words — who".
            ("testimonials" | "two-quotes", _, Some((k, part))) => {
                let item = self.item(k)?;
                let (words, who) = said(item);
                text(match part {
                    "" => Some(words),
                    "note" => who,
                    _ => None,
                })
            }
            // "Name, role: credit".
            ("team" | "team-four" | "team-six", _, Some((k, part))) if part != "art" => {
                let item = self.item(k)?;
                let label = item.label.clone().unwrap_or_else(|| item.whole.clone());
                let (person, role) =
                    label.split_once(", ").map_or((label.clone(), None), |(n, r)| (n.to_string(), Some(r.to_string())));
                text(match part {
                    "title" => Some(person),
                    "" => {
                        let detail = item.label.as_ref().map(|_| item.detail.clone()).filter(|d| !d.is_empty());
                        match (role, detail) {
                            (Some(r), Some(d)) => Some(format!("{r} \u{b7} {}", sentence(&d))),
                            (r, d) => r.or(d),
                        }
                    }
                    _ => None,
                })
            }
            // "task: lead / team / sponsor".
            ("raci", _, Some((k, part))) if part != "role" && part != "card" => {
                let item = self.item(k)?;
                let owners: Vec<String> =
                    item.detail.split(['/', ',']).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
                text(match part {
                    "task" => Some(item.label.clone().unwrap_or_else(|| item.whole.clone())),
                    "lead" => owners.first().cloned(),
                    "team" => owners.get(1).cloned(),
                    "sponsor" => owners.get(2).cloned(),
                    _ => None,
                })
            }
            // "item: amount", then "TOTAL: amount".
            ("receipt", "title", _) => text(self.title()),
            ("receipt", "total", _) => text(
                self.items
                    .iter()
                    .find(|i| i.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case("total")))
                    .map(|i| i.detail.clone()),
            ),
            ("receipt", _, Some((k, part))) => {
                let lines: Vec<&Item> = (self.items.iter())
                    .filter(|i| !i.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case("total")))
                    .collect();
                let item = lines.get(k)?;
                text(match part {
                    "item" => item.label.clone().or_else(|| Some(item.whole.clone())),
                    "amount" => item.label.is_some().then(|| item.detail.clone()),
                    _ => None,
                })
            }
            // "phase: does / thinks / feels".
            ("journey", _, _) if name.ends_with("-phase") => {
                let k = ORDINALS.iter().position(|o| name.starts_with(o))?;
                text(self.item(k).map(|i| i.label.clone().unwrap_or_else(|| i.whole.clone())))
            }
            ("journey", _, _) if ["does-", "thinks-", "feels-"].iter().any(|p| name.starts_with(p)) => {
                let (lane, ord) = name.split_once('-')?;
                let k = ORDINALS.iter().position(|o| *o == ord)?;
                let item = self.item(k)?;
                let p: Vec<String> = item.detail.split(" / ").map(|s| s.trim().to_string()).collect();
                let at = ["does", "thinks", "feels"].iter().position(|l| *l == lane)?;
                text(p.get(at).filter(|s| !s.is_empty()).map(|s| sentence(s)))
            }
            // "key result: target", the objective the header.
            ("okrs", _, Some((k, part))) => {
                let item = self.item(k)?;
                text(match part {
                    "" => item.label.clone().or_else(|| Some(item.whole.clone())),
                    "target" => item.label.is_some().then(|| item.detail.clone()),
                    _ => None,
                })
            }
            // "status: item".
            ("checklist", _, Some((k, part))) => {
                let item = self.item(k)?;
                let status = item.label.as_deref().filter(|l| l.split_whitespace().count() <= 2);
                text(match part {
                    "state" => status.map(str::to_string),
                    "" => Some(if status.is_some() { sentence(&item.detail) } else { item.whole.clone() }),
                    _ => None,
                })
            }
            // Each a quadrant's title, placed on the axes.
            ("matrix", _, Some((k, ""))) => text(self.item(k).map(|i| self.short(i))),
            // A picture's caption under it: the state, and what it is.
            ("before-after", _, Some((k, ""))) => text(self.item(k).map(|i| i.whole.clone())),
            // What you need, then the method's steps.
            ("recipe", "needs" | "method", _) => {
                let needed = |i: &Item| {
                    i.label.as_deref().is_some_and(|l| {
                        let l = l.to_lowercase();
                        ["need", "ingredient", "you need"].iter().any(|n| l.starts_with(n))
                    })
                };
                let (needs, method): (Vec<String>, Vec<String>) = if self.items.iter().any(needed) {
                    (
                        self.items.iter().filter(|i| needed(i)).map(|i| sentence(&i.detail)).collect(),
                        self.items.iter().filter(|i| !needed(i)).map(|i| sentence(&i.detail)).collect(),
                    )
                } else {
                    let half = self.items.len().div_ceil(2);
                    (self.wholes()[..half].to_vec(), self.wholes()[half..].to_vec())
                };
                let side = if name == "needs" { needs } else { method };
                let kind = if name == "needs" { "bullet" } else { "number" };
                Some((!side.is_empty()).then(|| Fill::List(side, list_kind.unwrap_or(kind))))
            }
            // A memo's and a letter's fields: whom it is to, what it is about, and its body; its
            // date is left out, which no model knows.
            ("memo", "about", _) => text(self.title()),
            ("memo" | "letter", "to", _) => text(self.subtitle()),
            ("memo" | "letter", "date", _) => Some(None),
            ("memo" | "letter", "from" | "sign", _) => Some(Some(Fill::Prompt)),
            ("memo" | "letter", "body", _) => text((!self.items.is_empty()).then(|| self.wholes().join("\n\n"))),
            ("headline", "tag" | "byline", _) => Some(None),
            ("the-ask", "ask", _) => text(self.title()),
            _ => None,
        }
    }

    /// The items for and against: those whose label is one of `pro`'s words, and the rest; else
    /// the first half and the second.
    fn sides(&self, pro: &[&str]) -> (Vec<String>, Vec<String>) {
        let is_pro = |i: &Item| i.label.as_deref().is_some_and(|l| pro.iter().any(|p| l.to_lowercase().starts_with(p)));
        let labelled = self.items.iter().all(|i| i.label.is_some());
        if labelled && self.items.iter().any(is_pro) {
            let (a, b): (Vec<&Item>, Vec<&Item>) = self.items.iter().partition(|i| is_pro(i));
            let details = |side: Vec<&Item>| side.iter().map(|i| sentence(&i.detail)).collect();
            return (details(a), details(b));
        }
        let half = self.items.len().div_ceil(2);
        (self.wholes()[..half].to_vec(), self.wholes()[half..].to_vec())
    }

    /// What a text in `slot` is to the slide's story: the slide's claim where it says its title,
    /// or a big number's meaning, a quote, a letter's body; a source; a signpost.
    fn semantic(&self, slot: &str, text: &str) -> Option<&'static str> {
        let claim = match (self.layout, slot) {
            ("big-number", "claim") | ("quote", "quote") | ("letter", "body") => true,
            _ => self.title().as_deref() == Some(text),
        };
        if claim {
            return Some("claim");
        }
        match slot {
            "takeaway" => Some("takeaway"),
            "under" | "note" if self.fact().as_deref() == Some(text) || text.starts_with("Source: ") => Some("source"),
            "who" => Some("source"),
            "kicker" => Some("navigation"),
            "number" if self.layout == "section" => Some("navigation"),
            _ => None,
        }
    }
}

/// Which ids a new one must not take.
#[derive(Clone, Copy)]
enum Taken {
    Node,
    State,
    Data,
    Section,
    Beat,
}

/// The deck as it is made.
#[derive(Default)]
struct Builder {
    nodes: Map<String, Value>,
    states: Vec<Value>,
    data: Map<String, Value>,
    sections: Vec<String>,
    beats: Vec<String>,
    seed: u64,
}

impl Builder {
    /// The first of `base`, `base-2`, … that no id of its kind takes; taken.
    fn unique(&mut self, base: &str, kind: Taken) -> String {
        let taken = |b: &Builder, id: &str| match kind {
            Taken::Node => b.nodes.contains_key(id),
            Taken::State => b.states.iter().any(|s| s["id"] == id),
            Taken::Data => b.data.contains_key(id),
            Taken::Section => b.sections.iter().any(|s| s == id),
            Taken::Beat => b.beats.iter().any(|s| s == id),
        };
        let id = (1..)
            .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
            .find(|id| !taken(self, id))
            .expect("some number is free");
        match kind {
            Taken::Section => self.sections.push(id.clone()),
            Taken::Beat => self.beats.push(id.clone()),
            _ => {}
        }
        id
    }

    /// A state's id: the title's first words, else its layout.
    fn state_id(&mut self, title: &str, layout: &str) -> String {
        let words: Vec<&str> = title.split_whitespace().take(5).collect();
        let base = if words.is_empty() { slug(layout, "slide") } else { slug(&words.join(" "), "slide") };
        self.unique(&base, Taken::State)
    }

    /// The slide `s` describes: its nodes, and its state showing them, in after a fade where
    /// another slide went before.
    fn slide(&mut self, s: Slots, after: bool) -> Result<SlideMade, OpsError> {
        let template = s
            .theme
            .layouts
            .get(s.layout)
            .ok_or_else(|| OpsError::new(format!("the theme has no layout `{}`", s.layout)))?;
        let presets = s.theme.shaders.as_ref().and_then(|sh| sh.presets.as_ref());
        let mut props = Map::new();
        let mut made = SlideMade { data: None };
        let mut claimed = false;
        for (name, slot) in &template.slots {
            let Some(fill) = s.fill(name, slot) else { continue };
            let id = self.unique(&format!("{}-{}", s.state, slug(name, "slot")), Taken::Node);
            let at = json!({ "in": name });
            let mut node = match fill {
                Fill::Text(text) => {
                    let Some(role) = &slot.role else { continue };
                    let mut node = json!({ "type": "text", "role": role, "text": text, "at": at });
                    match s.semantic(name, &text) {
                        Some("claim") if claimed => {}
                        Some(semantic) => {
                            claimed |= semantic == "claim";
                            node["semantic"] = json!(semantic);
                        }
                        None => {}
                    }
                    node
                }
                Fill::List(entries, kind) => {
                    let Some(role) = &slot.role else { continue };
                    if entries.is_empty() {
                        continue;
                    }
                    let list = vec![json!({ "kind": kind }); entries.len()];
                    json!({ "type": "text", "role": role, "text": entries.join("\n"), "list": list, "at": at })
                }
                Fill::Prompt => {
                    let (Some(role), Some((words, _))) = (&slot.role, slot.prompt.as_ref().and_then(Prompt::text))
                    else {
                        continue;
                    };
                    json!({ "type": "text", "role": role, "text": words, "at": at, "semantic": "navigation" })
                }
                Fill::Shape => {
                    let Some(Prompt::Shape(shape)) = &slot.prompt else { continue };
                    let mut node =
                        json!({ "type": "shape", "kind": shape.shape, "at": at, "z": -1, "semantic": "decoration" });
                    if let Some(fill) = &shape.fill {
                        node["fill"] = json!(fill);
                    }
                    if let Some(stroke) = &shape.stroke {
                        node["stroke"] = json!({ "paint": stroke, "width": "thin" });
                    }
                    if let Some(points) = &shape.points {
                        node["points"] = json!(points);
                    }
                    node
                }
                Fill::Picture => {
                    let wanted = match &slot.example {
                        Some(Example::Picture(p)) => p.as_str(),
                        _ => "backdrop",
                    };
                    let Some((preset, p)) = presets.and_then(|ps| ps.get_key_value(wanted).or_else(|| ps.first()))
                    else {
                        continue;
                    };
                    self.seed += 1;
                    // Under the cards (`z` −1) and the words over it, as a picture placed there lies.
                    let mut node = json!({ "type": "shader", "kind": p.kind, "preset": preset, "seed": self.seed, "at": at, "z": -2 });
                    if !s.slide.visual_description.is_empty() {
                        node["alt"] = json!(s.slide.visual_description);
                    }
                    node
                }
                Fill::Chart => {
                    let Some(chart) = &s.slide.chart_data else { continue };
                    let data = self.unique(&format!("{}-data", s.state), Taken::Data);
                    self.data.insert(data.clone(), source(chart));
                    made.data = Some(data.clone());
                    chart_node(chart, &data, s.layout, &s.slide.slide_title, at)
                }
            };
            if node.get("semantic").is_none() && node["type"] == "text" && !claimed && s.title().is_none() {
                // A slide with no title says what it is in its first words.
                node["semantic"] = json!("claim");
                claimed = true;
            }
            self.nodes.insert(id.clone(), node);
            props.insert(id, json!({}));
        }
        let mut state = json!({ "id": s.state, "mode": "absolute", "layout": s.layout, "props": props });
        if after {
            state["transition"] = json!({ "duration": "standard", "ease": "standard" });
        }
        self.states.push(state);
        Ok(made)
    }
}

/// A chart's figures as a data source, written inline: a row for each category and series.
fn source(chart: &ChartData) -> Value {
    let several = chart.series.len() > 1;
    let mut rows = Vec::new();
    for (c, category) in chart.categories.iter().enumerate() {
        for (n, series) in chart.series.iter().enumerate() {
            let Some(value) = series.values.get(c) else { continue };
            let mut row = json!({ "category": category, "value": value });
            if several {
                row["series"] = json!(series.name.clone().unwrap_or_else(|| format!("Series {}", n + 1)));
            }
            rows.push(row);
        }
    }
    let mut schema = json!({ "category": "string", "value": "number" });
    if several {
        schema["series"] = json!("string");
    }
    json!({ "source": { "inline": rows }, "schema": schema })
}

/// The chart that plots `chart`'s source, `data`, in a layout's chart slot.
fn chart_node(chart: &ChartData, data: &str, layout: &str, title: &str, at: Value) -> Value {
    let kind = if layout == "line-chart" { "line" } else { "bar" };
    let several = chart.series.len() > 1;
    let y_title = match (chart.series.first().and_then(|s| s.name.as_deref()), chart.unit.as_deref()) {
        (Some(name), Some(unit)) if !several && !name.contains(unit) => Some(format!("{name} ({unit})")),
        (Some(name), _) if !several => Some(name.to_string()),
        (_, Some(unit)) => Some(unit.to_string()),
        _ => None,
    };
    let mut y = json!({ "field": "value", "type": "quantitative" });
    if let Some(t) = y_title {
        y["title"] = json!(t);
    }
    if kind == "bar" {
        y["domain"] = json!([0, null]);
    }
    let mut node = json!({
        "type": "chart",
        "kind": kind,
        "data": format!("@{data}"),
        "x": { "field": "category", "type": "ordinal" },
        "y": y,
        "alt": title,
        "semantic": "evidence",
        "at": at,
    });
    if several {
        node["series"] = json!({ "field": "series", "type": "nominal" });
        node["color"] = json!({ "field": "series", "type": "nominal", "scale": "categorical" });
        node["legend"] = json!("top");
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_narrato::{Brief, answer};

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The theme that ships as Dusk, and its fonts, read where the repository keeps them.
    fn dusk() -> String {
        std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap()
    }

    fn font(file: &str) -> Result<Vec<u8>, OpsError> {
        Ok(std::fs::read(root().join("docs/examples").join(file))?)
    }

    fn narrative(summary: &str, slides: Vec<Slide>) -> Narrative {
        Narrative {
            arc: "Problem\u{2013}Solution".into(),
            arc_reasoning: "A problem the room knows, then its answer.".into(),
            executive_summary: summary.into(),
            slides,
            truncated: false,
            brief: Brief {
                topic: "Pitch a Series A round for our carbon-negative cargo platform.".into(),
                audience: "Investors".into(),
                goal: "Persuade".into(),
                ..Brief::default()
            },
        }
    }

    /// A slide in each of the 56 layouts Narrato's catalog names, written as Scaena's prompt
    /// asks, in the catalog's order but the closing last.
    fn catalog() -> (String, Vec<Slide>) {
        let text = std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/catalog.json")).unwrap();
        let written = answer::slides(&text).unwrap();
        assert_eq!(written.slides.len(), 56);
        (written.executive_summary, written.slides)
    }

    /// The deck's nodes in `state`, by slot: what each says, or its type.
    fn shown(deck: &Value, state: &str) -> Vec<(String, String)> {
        let state = deck["states"].as_array().unwrap().iter().find(|s| s["id"] == state).unwrap();
        let mut out = Vec::new();
        for id in state["props"].as_object().unwrap().keys() {
            let node = &deck["nodes"][id];
            let slot = node["at"]["in"].as_str().unwrap().to_string();
            let said = node["text"].as_str().map_or_else(|| node["type"].as_str().unwrap().to_string(), str::to_string);
            out.push((slot, said));
        }
        out
    }

    #[test]
    fn an_item_reads_its_label_and_its_figure() {
        let i = item("\u{2022} $4.1M: Q3 revenue, up 12%");
        assert_eq!((i.label.as_deref(), i.detail.as_str()), (Some("$4.1M"), "Q3 revenue, up 12%"));
        assert!(i.figure_first());
        assert_eq!((i.figure().as_deref(), i.name().as_deref()), (Some("$4.1M"), Some("Q3 revenue")));
        let i = item("Our wedge \u{2014} the only carbon-negative lane");
        assert_eq!(i.label.as_deref(), Some("Our wedge"));
        assert_eq!(item("No label here").label, None);
        assert_eq!(figure("Revenue grew 41% in pilots").as_deref(), Some("41%"));
        assert_eq!(figure("a $12.5M round").as_deref(), Some("$12.5M"));
        assert_eq!(figure("3\u{d7} faster").as_deref(), Some("3\u{d7}"));
        assert_eq!(figure("in 2024, then").as_deref(), Some("2024"));
        assert_eq!(figure("5Mbps").as_deref(), Some("5"));
        assert_eq!(figure("none"), None);
        assert!(numeric("$4.1M") && numeric("42%") && numeric("6 weeks") && numeric("2024"));
        assert!(!numeric("Q3 revenue") && !numeric("H1 2027") && !numeric("3 ships a week"));
        assert_eq!(head("Q3 revenue, up 12%"), ("Q3 revenue".to_string(), "up 12%".to_string()));
        assert_eq!(after_figure("1,200, 28% go on").as_deref(), Some("28% go on"));
        assert_eq!(after_figure("$40M revenue by 2028"), None);
        assert_eq!(after_figure("41%").as_deref(), Some(""));
    }

    #[test]
    fn each_layout_narrato_names_is_set_in_the_themes_own() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let wanted = [
            "agenda",
            "section",
            "big-number",
            "statement",
            "quote",
            "bar-chart",
            "line-chart",
            "scorecard",
            "number-grid",
            "comparison",
            "before-after",
            "pros-cons",
            "quadrants",
            "steps",
            "timeline",
            "journey",
            "full",
            "art-right",
            "three-up",
            "cycle",
            "callout",
            "roadmap",
            "matrix",
            "okrs",
            "raci",
            "swot",
            "thesis",
            "letter",
            "memo",
            "receipt",
            "next-steps",
            "milestones",
            "three-horizons",
            "funnel",
            "scenarios",
            "head-to-head",
            "ranking",
            "headline",
            "manifesto",
            "team-four",
            "bio",
            "testimonials",
            "how-it-works",
            "pipeline",
            "stack",
            "faq",
            "glossary",
            "checklist",
            "the-ask",
            "qa",
            "recipe",
            "definition",
            "numbered",
            "bullets",
        ];
        let (cover, closing) = (slides[0].clone(), slides[55].clone());
        let mut wrong = Vec::new();
        for (slide, wanted) in slides[1..55].iter().zip(wanted) {
            let made = deck(&narrative(&summary, vec![cover.clone(), slide.clone(), closing.clone()]), &theme).unwrap();
            assert_eq!((made.layouts[0].as_str(), made.layouts[2].as_str()), ("title", "closing"));
            if made.layouts[1] != wanted {
                wrong.push(format!("{}: {} (wanted {wanted})", slide.layout_suggestion, made.layouts[1]));
            }
            let state = made.deck["states"][1]["id"].as_str().unwrap();
            let nodes = shown(&made.deck, state);
            assert!(nodes.len() >= 2, "{wanted}: {nodes:?}");
            assert!(nodes.iter().all(|(_, said)| !said.trim().is_empty()), "{wanted}: {nodes:?}");
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn each_place_of_a_layout_takes_its_part_of_an_item() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let made = |name: &str| {
            let slide = slides.iter().find(|s| s.layout_suggestion == name).unwrap().clone();
            let made = deck(&narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]), &theme).unwrap();
            let state = made.deck["states"][1]["id"].as_str().unwrap().to_string();
            let mut said: Vec<(String, String)> = shown(&made.deck, &state);
            said.sort();
            said
        };
        let has = |said: &[(String, String)], slot: &str, words: &str| {
            assert!(said.iter().any(|(s, w)| s == slot && w == words), "{slot} = {words:?} in {said:#?}");
        };
        let scorecard = made("Scorecard");
        has(&scorecard, "first-figure", "$2.4M");
        has(&scorecard, "first-title", "Offsets bought each year");
        has(&scorecard, "first", "Up 31%");
        has(&scorecard, "third-figure", "6 weeks");
        let pipeline = made("Pipeline");
        has(&pipeline, "first-figure", "1,200");
        has(&pipeline, "first-title", "Leads this year");
        has(&pipeline, "first", "12% go on");
        let ranking = made("Ranking");
        has(&ranking, "first-rank", "1");
        has(&ranking, "first-figure", "41%");
        has(&ranking, "first", "Houston to Tampa");
        let timeline = made("Timeline");
        has(&timeline, "first-title", "2023");
        has(&timeline, "first", "Capture prototype proves 92% exhaust uptake");
        let vs = made("VS");
        has(&vs, "first-title", "Tidewater");
        has(&vs, "second-title", "Offsets");
        has(&vs, "first-criterion", "Verification");
        has(&vs, "first-left", "Per-container certificate");
        has(&vs, "first-right", "Self-reported");
        let okrs = made("OKRs");
        has(&okrs, "header", "Make the Gulf corridor the default for green freight");
        has(&okrs, "kicker", "Q4 OKRs");
        has(&okrs, "first", "Shippers under contract");
        has(&okrs, "first-target", "50");
        let number = made("Big Number");
        has(&number, "number", "3%");
        has(&number, "claim", "of global CO\u{2082} comes from shipping, more than all of aviation");
        has(&number, "under", "International Maritime Organization, Fourth GHG Study");
        let journey = made("Journey");
        has(&journey, "first-phase", "Discover");
        has(&journey, "feels-second", "Cautiously impressed");
        let checklist = made("Checklist");
        has(&checklist, "third-state", "In progress");
        has(&checklist, "third", "Two vessels retrofitted");
        let receipt = made("Receipt");
        has(&receipt, "fourth-item", "Verification");
        has(&receipt, "total", "$41");
        let team = made("Team");
        has(&team, "second-title", "Ade Okafor");
        has(&team, "second", "CTO \u{b7} Built direct air capture at scale");
        let before = made("Before / After");
        has(&before, "first", "Before: buy offsets, file reports, and hope");
        let matrix = made("2\u{d7}2 Matrix");
        has(&matrix, "third", "Dashboards");
        let steps = made("Steps");
        has(&steps, "first", "The shipper reserves a carbon-negative lane");
        let section = made("Section Header");
        has(&section, "number", "01");
        assert_eq!(sentence("iPhone sales"), "iPhone sales");
        assert_eq!(sentence("cuts emissions 80%"), "Cuts emissions 80%");
    }

    #[test]
    fn a_layout_with_no_place_for_an_item_gives_way_to_a_list() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        assert!(holds(&theme, "statement", 1) && !holds(&theme, "statement", 4));
        assert!(holds(&theme, "steps", 4) && !holds(&theme, "steps", 6));
        assert!(holds(&theme, "pros-cons", 7) && holds(&theme, "bullets", 12) && holds(&theme, "bar-chart", 3));
        // A head-to-head's first item names the sides; a receipt's last is its total.
        assert!(holds(&theme, "head-to-head", 4) && !holds(&theme, "head-to-head", 5));
        assert!(holds(&theme, "receipt", 5) && !holds(&theme, "receipt", 6));
        let (summary, slides) = catalog();
        let mut steps = slides.iter().find(|s| s.layout_suggestion == "Steps").unwrap().clone();
        steps.key_bullets.extend(["Report: certificates reach the buyer".into(), "Renew: the lane books again".into()]);
        let made = deck(&narrative(&summary, vec![slides[0].clone(), steps, slides[55].clone()]), &theme).unwrap();
        assert_eq!(made.layouts[1], "numbered");
    }

    #[test]
    fn a_recorded_answer_makes_a_deck_that_validates() {
        use scaena_narrato::{Generation, Provider, Settings, Step};
        let answer = |name: &str| {
            std::fs::read_to_string(root().join(format!("crates/scaena-narrato/tests/answers/{name}.json"))).unwrap()
        };
        let settings = Settings { provider: Provider::Anthropic, model: "a-model".into(), base: None, browser: false };
        let mut g = Generation::new(narrative("", Vec::new()).brief, settings).unwrap();
        assert_eq!(g.answer(200, "OK", &answer("anthropic-arc")).unwrap(), Step::Slides);
        assert_eq!(g.answer(200, "OK", &answer("anthropic-slides")).unwrap(), Step::Done);
        let n = g.into_narrative().unwrap();
        let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        let deck = write.unwrap().deck;
        assert_eq!(deck.states.len(), n.slides.len());
        assert_eq!(deck.meta.as_ref().and_then(|m| m.description.as_deref()), Some(n.executive_summary.as_str()));
        // Each slide a beat, its title the claim, the chart's figures its evidence.
        let beats: Vec<_> = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).collect();
        assert_eq!(beats.len(), n.slides.len());
        assert_eq!(beats[0].claim, n.slides[0].slide_title);
        assert!(beats.iter().any(|b| b.evidence.iter().any(|e| e.starts_with('@'))));
    }

    #[test]
    fn a_catalog_of_every_layout_makes_a_deck_that_lints_clean() {
        let (summary, slides) = catalog();
        let n = narrative(&summary, slides);
        let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created, "{:#?}", made.created.findings);
        let write = write.unwrap();
        let errors: Vec<&Finding> =
            made.created.findings.iter().filter(|f| f.severity == scaena_core::Severity::Error).collect();
        for f in &made.created.findings {
            eprintln!("{} {:?} {}", f.code, f.path, f.message);
        }
        eprintln!("fixed {}", made.fixed.len());
        if let Ok(dir) = std::env::var("SCAENA_NARRATO_OUT") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let b = crate::Bundle {
                root: dir.clone(),
                deck_file: "deck.json".into(),
                deck: write.deck.clone(),
                theme_json: None,
                files: scaena_store::Files::Dir(dir),
                author: "user".into(),
            };
            crate::lint::write(&b, write.clone()).unwrap();
        }
        assert!(errors.is_empty(), "{errors:#?}");
        assert_eq!(write.deck.states.len(), 56);
    }
}
