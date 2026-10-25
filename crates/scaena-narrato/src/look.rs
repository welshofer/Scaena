//! A deck seen as it is drawn (PLAN G.13): each slide drawn, shown to the writing model beside
//! what may change on it, and the changes it asks for read back. Scaena's own call, made once
//! the deck is made, as the fit is ([`crate::fit`]): Narrato writes slides it never sees, and a
//! layout engine sets words it never reads, so the model looks at what they made together, as
//! an art director at a wall of prints.
//!
//! The model changes nothing itself. It picks, for a slide, another of the layouts the builder
//! offers it, and tightens its words; the builder makes the deck again from them, and keeps
//! each slide's change only where lint finds it no worse.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A layout a slide may take, as the model is told of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// Its name in the theme, which the answer gives back.
    pub id: String,
    /// What a person calls it.
    pub name: String,
    /// What it draws, in a few words.
    pub draws: String,
}

/// A slide as the model is shown it: its number, from 1; the layout it is drawn in and the
/// others it may take; and its words as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub slide: usize,
    pub layout: Choice,
    pub options: Vec<Choice>,
    pub title: String,
    pub subtitle: String,
    pub bullets: Vec<String>,
}

/// What the model asks to change on a slide: another of its layouts, and its words tightened,
/// each kept where [`read`] finds it one the slide may take.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Change {
    pub slide: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullets: Option<Vec<String>>,
    /// What the model saw, in a sentence.
    pub why: String,
}

impl Change {
    /// Whether it changes anything.
    pub fn changes(&self) -> bool {
        self.layout.is_some() || self.title.is_some() || self.subtitle.is_some() || self.bullets.is_some()
    }
}

/// The rider the look is asked under.
pub const LOOK_SYSTEM: &str = "You are the art director of a presentation studio, reviewing a deck a layout engine set from a writer's draft. You see every slide as it will be shown. Output ONLY a JSON object \u{2014} no markdown, no code fence, no commentary \u{2014} with one key, \"slides\": an array with one object for each slide you change, each with \"slide\" (integer, its number), \"layout\" (string: one of the other layouts that slide may take, by its id, or \"\" to keep its own), \"title\" (string: its title tightened, or \"\" to keep it), \"subtitle\" (string: its subtitle tightened, or \"\" to keep it), \"bullets\" (array of strings: every one of its bullets, tightened, in order, as many as it has; or [] to keep them), and \"why\" (string: what you saw, in one sentence).";

/// What is said of a slide before its picture: its number, its layout and the others it may
/// take, and its words as written.
pub fn caption(seen: &Seen, count: usize) -> String {
    let mut out = format!("Slide {} of {count}, drawn as {} ({}).", seen.slide, seen.layout.name, seen.layout.draws);
    if seen.options.is_empty() {
        out.push_str(" It keeps its layout.");
    } else {
        out.push_str(" It may instead take:");
        for (k, o) in seen.options.iter().enumerate() {
            let sep = if k == 0 { " " } else { "; " };
            out.push_str(&format!("{sep}`{}`, {} ({})", o.id, o.name, o.draws));
        }
        out.push('.');
    }
    if !seen.title.is_empty() {
        out.push_str(&format!("\nTitle: {}", seen.title));
    }
    if !seen.subtitle.is_empty() {
        out.push_str(&format!("\nSubtitle: {}", seen.subtitle));
    }
    for (k, b) in seen.bullets.iter().enumerate() {
        out.push_str(&format!("\nBullet {}: {b}", k + 1));
    }
    out
}

/// The prompt after the slides: what to look for, what may change, and what may not, for a
/// deck on `topic` for `audience`, in `language` where it is written in one. `shown` is the
/// first and last slide shown, of the deck's `count`: a long deck is shown a dozen slides at a
/// time (`scaena_ops::narrative::LOOK_BATCH`), and the prompt says which (narrative PERF-5).
pub fn prompt(topic: &str, audience: &str, count: usize, language: Option<&str>, shown: (usize, usize)) -> String {
    let topic = topic.trim();
    let mut out = if shown == (1, count) {
        format!("You have seen the {count} slides of a deck on \u{201c}{topic}\u{201d}")
    } else {
        format!(
            "You have seen slides {}\u{2013}{} of the {count} slides of a deck on \u{201c}{topic}\u{201d}",
            shown.0, shown.1
        )
    };
    if !audience.trim().is_empty() {
        out.push_str(&format!(", for {}", audience.trim()));
    }
    out.push_str(
        ", each drawn as it will be projected. Change only what would make a slide read better from the back of the room:\n\
         - Words too small to read, or crowded: tighten them, or move the slide to one of its other layouts that gives them more room.\n\
         - Much of a slide left empty, or its words crowded into one part of it: one of its other layouts that fills it.\n\
         - Three or more slides in a row drawn alike: give one of them another of its layouts.\n\
         - A title that runs past two lines: tighten it to one idea.\n\
         - Items of one slide said in unlike forms (one with a detail, the others none; one a sentence, the others a word): rewrite them in one form.\n\
         Keep every figure, name, date, and claim a slide makes, and add none. A bullet written \u{201c}Label: detail\u{201d} stays so written, its label first. Keep a slide's bullets as many as they are, in their order, none longer than it was by more than a few characters. Leave alone each slide that reads well: most should. List only the slides you change.",
    );
    if let Some(language) = language.filter(|l| !l.is_empty() && *l != "English") {
        out.push_str(&format!(" Write in {language}, as the deck is written."));
    }
    out
}

/// What is said of the slides just outside a round, so that "three or more slides in a row
/// drawn alike" holds across a round's edge (lift-up PERF-1): nothing for a deck shown whole.
pub fn edges(edges: [Option<&Seen>; 2]) -> String {
    let said = |s: &Seen| format!("slide {} is drawn as {}", s.slide, s.layout.name);
    match edges {
        [None, None] => String::new(),
        [Some(b), None] => format!(" Just before these, {}.", said(b)),
        [None, Some(a)] => format!(" Just after these, {}.", said(a)),
        [Some(b), Some(a)] => format!(" Just before these, {}; just after them, {}.", said(b), said(a)),
    }
}

/// The answer's contract, as Gemini takes it (the others derive theirs from it).
pub fn schema_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "slides": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "slide": { "type": "INTEGER" },
                        "layout": { "type": "STRING" },
                        "title": { "type": "STRING" },
                        "subtitle": { "type": "STRING" },
                        "bullets": { "type": "ARRAY", "items": { "type": "STRING" } },
                        "why": { "type": "STRING" }
                    },
                    "required": ["slide", "layout", "title", "subtitle", "bullets", "why"]
                }
            }
        },
        "required": ["slides"]
    })
}

/// How much longer than its words a rewrite may run: a few characters, for words set in
/// another order.
fn longest(was: &str) -> usize {
    let n = was.chars().count();
    n + (n / 10).max(4)
}

/// Whether a bullet is written "Label: detail".
fn labelled(bullet: &str) -> bool {
    bullet.find(": ").is_some_and(|at| (1..=60).contains(&bullet[..at].chars().count()))
}

/// The changes an answer asks for, each kept where it names a slide shown, changes something,
/// and keeps to what the slide may take: one of its other layouts; words no longer than they
/// were by more than [`longest`] allows; as many bullets, each labelled where it was. The first
/// change a slide is given stands. Words or bullets that change a figure are not taken (REL-2).
pub fn read(text: &str, seen: &[Seen]) -> Result<Vec<Change>, String> {
    let start = text.find('{').ok_or("the answer holds no JSON object")?;
    let end = text.rfind('}').ok_or("the answer holds no JSON object")?;
    let value: Value =
        serde_json::from_str(&text[start..=end]).map_err(|e| format!("the answer is not the JSON asked for: {e}"))?;
    let slides = value.get("slides").and_then(Value::as_array).ok_or("the answer has no `slides`")?;
    let mut out: Vec<Change> = Vec::new();
    for asked in slides {
        let Some(number) = asked.get("slide").and_then(Value::as_u64).map(|n| n as usize) else { continue };
        let Some(s) = seen.iter().find(|s| s.slide == number) else { continue };
        if out.iter().any(|c| c.slide == number) {
            continue;
        }
        let said = |key: &str| asked.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
        let words = |key: &str, was: &str| {
            let now = said(key);
            (!now.is_empty() && now != was && now.chars().count() <= longest(was) && crate::same_figures(was, &now))
                .then_some(now)
        };
        let layout = Some(said("layout")).filter(|l| s.options.iter().any(|o| &o.id == l));
        let bullets = asked
            .get("bullets")
            .and_then(Value::as_array)
            .map(|b| b.iter().filter_map(Value::as_str).map(|b| b.trim().to_string()).collect::<Vec<_>>())
            .filter(|now| {
                now.len() == s.bullets.len()
                    && now != &s.bullets
                    && now.iter().zip(&s.bullets).all(|(now, was)| {
                        !now.is_empty()
                            && now.chars().count() <= longest(was)
                            && labelled(now) == labelled(was)
                            && crate::same_figures(was, now)
                    })
            });
        let change = Change {
            slide: number,
            layout,
            title: words("title", &s.title),
            subtitle: words("subtitle", &s.subtitle).filter(|_| !s.subtitle.is_empty()),
            bullets,
            why: said("why"),
        };
        if change.changes() {
            out.push(change);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(id: &str, name: &str, draws: &str) -> Choice {
        Choice { id: id.into(), name: name.into(), draws: draws.into() }
    }

    fn seen() -> Vec<Seen> {
        vec![
            Seen {
                slide: 1,
                layout: choice("title-center", "Centered Title", "the deck's title"),
                options: Vec::new(),
                title: "Climate Change: Where We Stand".into(),
                subtitle: "What the data says".into(),
                bullets: Vec::new(),
            },
            Seen {
                slide: 2,
                layout: choice("bullets", "Bullets", "a list"),
                options: vec![
                    choice("numbered-cards", "Numbered Cards", "each item on a card under its number"),
                    choice("quadrants", "Quadrants", "four items, two by two"),
                ],
                title: "The Path to Net Zero by the Middle of the Century".into(),
                subtitle: String::new(),
                bullets: vec![
                    "Clean the grid: triple renewables by 2030".into(),
                    "Electrify: vehicles, heat pumps, industrial heat".into(),
                    "Cut methane: 75% from fossil fuel operations".into(),
                    "Remove the rest: forests, soils, direct capture".into(),
                ],
            },
        ]
    }

    #[test]
    fn each_slide_is_captioned_with_its_layouts_and_words() {
        let s = seen();
        let c = caption(&s[1], 25);
        assert!(c.starts_with("Slide 2 of 25, drawn as Bullets (a list). It may instead take: `numbered-cards`, Numbered Cards (each item on a card under its number); `quadrants`"), "{c}");
        assert!(c.contains("\nTitle: The Path to Net Zero") && c.contains("\nBullet 4: Remove the rest"));
        assert!(caption(&s[0], 25).contains("It keeps its layout.\nTitle: Climate Change"));
        let p = prompt("Climate change", "colleagues", 25, Some("Czech"), (1, 25));
        assert!(
            p.starts_with("You have seen the 25 slides of a deck on \u{201c}Climate change\u{201d}, for colleagues")
        );
        assert!(p.contains("Keep every figure") && p.ends_with("Write in Czech, as the deck is written."));
        // A long deck shown a dozen at a time says which slides these are (narrative PERF-5).
        let part = prompt("Climate change", "", 25, None, (13, 24));
        assert!(part.starts_with("You have seen slides 13\u{2013}24 of the 25 slides of a deck on \u{201c}Climate change\u{201d}, each drawn"), "{part}");
        // The slides just outside a round, so a run drawn alike is seen across its edge (PERF-1).
        assert_eq!(edges([None, None]), "");
        assert_eq!(edges([Some(&s[0]), None]), " Just before these, slide 1 is drawn as Centered Title.");
        assert_eq!(
            edges([Some(&s[0]), Some(&s[1])]),
            " Just before these, slide 1 is drawn as Centered Title; just after them, slide 2 is drawn as Bullets."
        );
    }

    #[test]
    fn a_change_is_kept_where_the_slide_may_take_it() {
        let answer = r#"Here: {"slides": [
            {"slide": 2, "layout": "numbered-cards", "title": "The Path to Net Zero", "subtitle": "", "bullets": [], "why": "A wall of words; four steps read as cards."},
            {"slide": 2, "layout": "quadrants", "title": "", "subtitle": "", "bullets": [], "why": "Given twice: the first stands."},
            {"slide": 1, "layout": "bullets", "title": "", "subtitle": "", "bullets": [], "why": "Not one of its layouts."},
            {"slide": 9, "layout": "", "title": "No such slide", "subtitle": "", "bullets": [], "why": ""}
        ]}"#;
        let read = read(answer, &seen()).unwrap();
        assert_eq!(read.len(), 1, "{read:#?}");
        assert_eq!(read[0].layout.as_deref(), Some("numbered-cards"));
        assert_eq!(read[0].title.as_deref(), Some("The Path to Net Zero"));
        assert_eq!((read[0].subtitle.as_ref(), read[0].bullets.as_ref()), (None, None));
        assert!(super::read("no json", &seen()).is_err());
    }

    #[test]
    fn bullets_are_kept_as_many_labelled_where_they_were_and_hardly_longer() {
        let s = seen();
        let tightened = vec![
            "Clean the grid: triple renewables by 2030".to_string(),
            "Electrify: cars, heat, industry".to_string(),
            "Cut methane: 75% from fossil fuels".to_string(),
            "Remove the rest: forests, soils, capture".to_string(),
        ];
        let answer = |bullets: &[String]| {
            json!({ "slides": [{ "slide": 2, "layout": "", "title": "", "subtitle": "", "bullets": bullets, "why": "w" }] })
                .to_string()
        };
        assert_eq!(read(&answer(&tightened), &s).unwrap()[0].bullets.as_ref(), Some(&tightened));
        // One fewer, a label lost, or one much longer: none of them taken.
        assert!(read(&answer(&tightened[..3]), &s).unwrap().is_empty());
        let mut unlabelled = tightened.clone();
        unlabelled[1] = "Electrify cars, heat, and industry".into();
        assert!(read(&answer(&unlabelled), &s).unwrap().is_empty());
        let mut longer = tightened.clone();
        longer[0] = "Clean the grid: triple renewables by 2030 and double storage, worldwide, every year".into();
        assert!(read(&answer(&longer), &s).unwrap().is_empty());
        // A figure rounded, or dropped, is not the slide's evidence tightened (REL-2).
        let mut rounded = tightened.clone();
        rounded[2] = "Cut methane: about 70% from fossil fuels".into();
        assert!(read(&answer(&rounded), &s).unwrap().is_empty());
        // A subtitle where the slide has none is not one to tighten.
        let given = json!({ "slides": [{ "slide": 2, "layout": "", "title": "", "subtitle": "New words", "bullets": [], "why": "" }] });
        assert!(read(&given.to_string(), &s).unwrap().is_empty());
    }
}
