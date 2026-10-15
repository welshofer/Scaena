//! A haiku asked, counted, asked once more, and placed in one patch (PLAN O.4d, ADR-0030
//! decisions 26–29). The person's model writes it (`scaena_core::writing::haiku`), the engine
//! counts it (`scaena_core::verse`), and the theme sets it in its `verse` role: on a new Haiku
//! slide, in the slide's empty verse place, over the verse selected, or where the pointer last
//! pressed. Which model wrote it, with whose key, when, and about what, is in the slide's notes,
//! never on the slide.

use crate::{Context, OpsError};
use scaena_core::Deck;
use scaena_core::model::theme::Theme;
use scaena_core::verse::{self, Checked, Kept};
use scaena_core::writing::haiku::{self, About, Taken};
use scaena_core::writing::replacing;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The layout a new haiku's slide is started in, and its places.
pub const LAYOUT: &str = "haiku";
pub const POEM: &str = "poem";
pub const ATTRIBUTION: &str = "attribution";

/// A poem as the model wrote it and as the engine counts it: its lines, a Japanese poem's kana
/// reading, and each line's count against 5-7-5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Poem {
    pub lines: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<Vec<String>>,
    pub checked: Checked,
}

impl Poem {
    /// `taken` counted in `lang`: a Japanese poem by its reading, where it gave one.
    pub fn counted(taken: Taken, lang: &str) -> Poem {
        let japanese = lang.split(['-', '_']).next().is_some_and(|c| c.eq_ignore_ascii_case("ja"));
        let counted = match (&taken.reading, japanese) {
            (Some(reading), true) => verse::check(&reading.join("\n"), "ja"),
            _ => verse::check(&taken.lines.join("\n"), lang),
        };
        // Each line shows as written, its count the reading's where that is what counted.
        let mut checked = verse::check(&taken.lines.join("\n"), lang);
        if checked.lines.len() == counted.lines.len() {
            for (line, by) in checked.lines.iter_mut().zip(&counted.lines) {
                line.count = by.count;
            }
            checked.kept = counted.kept;
        }
        Poem { lines: taken.lines, reading: taken.reading, checked }
    }
}

/// What a round's answer comes to: a poem to offer, with a note where it does not count 5, 7, and
/// 5, or the second ask.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "next", rename_all = "lowercase")]
pub enum Answered {
    Offer {
        poem: Poem,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    Again {
        prompt: String,
    },
}

/// The rounds, at most two calls: the first answer is offered when it is kept or uncounted; else
/// it is asked once more, and the second is offered when kept; else the closer of the two, the
/// second on a tie, with a note in plain words.
pub fn answered(about: &About, lang: &str, first: Poem, second: Option<Poem>) -> Answered {
    let Some(second) = second else {
        if matches!(first.checked.kept, Kept::Kept | Kept::Uncounted) {
            return Answered::Offer { poem: first, note: None };
        }
        return Answered::Again { prompt: haiku::again(about, lang, &first.checked) };
    };
    if second.checked.kept == Kept::Kept {
        return Answered::Offer { poem: second, note: None };
    }
    let far = |p: &Poem| p.checked.distance().unwrap_or(u32::MAX);
    let poem = if far(&first) < far(&second) { first } else { second };
    let note = note(&poem.checked, lang);
    Answered::Offer { poem, note }
}

/// What a poem offered off 5-7-5 says under it: "Asked twice, the closest counts 5, 8, and 5
/// syllables, not 5, 7, and 5."
pub fn note(checked: &Checked, lang: &str) -> Option<String> {
    if matches!(checked.kept, Kept::Kept | Kept::Uncounted) {
        return None;
    }
    let unit = if lang.split(['-', '_']).next().is_some_and(|c| c.eq_ignore_ascii_case("ja")) {
        "morae"
    } else {
        "syllables"
    };
    let counts: Vec<String> = checked.lines.iter().map(|l| haiku::said(l.count)).collect();
    let counts = match counts.as_slice() {
        [a, b, c] => format!("{a}, {b}, and {c}"),
        _ => format!("{} lines", counts.len()),
    };
    Some(format!("Asked twice, the closest counts {counts} {unit}, not 5, 7, and 5."))
}

/// Who wrote a haiku, as its slide's notes say it: the model the person's key reached and whose
/// key, or a device's own model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Writer {
    /// The model's name, as the provider gave it.
    pub model: String,
    /// The provider whose key reached it (`Anthropic`); none for a model on the device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// The line a haiku adds to its slide's notes: "Haiku written by {model} with your {provider} key
/// on {date}, about {about}." `date` is the client's, as it says a day.
pub fn credited(by: &Writer, date: &str, about: &About) -> String {
    let with = by.provider.as_deref().map(|p| format!(" with your {p} key")).unwrap_or_default();
    format!("Haiku written by {}{with} on {date}, about {}.", by.model, about.said())
}

/// Where a haiku goes, as the person chose it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "lowercase")]
pub enum Place {
    /// A new Haiku slide after the slide shown, in its beat.
    New,
    /// The slide's empty verse place.
    Slot { slot: String },
    /// Over the verse text selected, its runs kept.
    Over { node: String },
}

/// A haiku placed: the patch, one step to undo, the state it is on, and its text node.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placed {
    pub patch: Vec<Value>,
    pub state: String,
    pub node: String,
}

/// The patch that places `lines` from `state`, as `place` says, with `attribution` where the
/// person gave one, and `credit` added to the notes of the state it is on (PLAN O.4d). A new slide
/// is started in the theme's Haiku layout (`states::starting`), its verse place given the poem and
/// its attribution's place left waiting where none is given; a slot is filled
/// (`states::filling`); a verse selected takes the poem by the least changes
/// (`writing::replacing`), the looks of the words it keeps kept.
pub fn placing(
    deck: &Deck,
    theme: &Theme,
    state: &str,
    lines: &[String],
    attribution: Option<&str>,
    place: &Place,
    credit: &str,
) -> Result<Placed, OpsError> {
    let poem = lines.iter().map(|l| l.trim()).collect::<Vec<_>>().join("\n");
    if poem.trim().is_empty() {
        return Err(OpsError::new("the haiku has no words"));
    }
    let attribution = attribution.map(str::trim).filter(|a| !a.is_empty());
    let (mut patch, on, node) = match place {
        Place::New => {
            if !theme.layouts.contains_key(LAYOUT) {
                return Err(OpsError::new("the theme has no Haiku layout"));
            }
            let started = crate::states::starting(deck, theme, state, Some(LAYOUT))?;
            let slotted = |op: &Value, slot: &str| {
                op["op"] == "add_node" && op["node"]["at"]["in"] == slot && op["node"].get("at").is_some()
            };
            let poem_id = (started.patch.iter().find(|op| slotted(op, POEM)))
                .and_then(|op| op["id"].as_str())
                .map(str::to_string)
                .ok_or_else(|| OpsError::new("the Haiku layout has no verse place"))?;
            let unsaid: Option<String> = match attribution {
                Some(_) => None,
                None => started.patch.iter().find(|op| slotted(op, ATTRIBUTION)).and_then(|op| op["id"].as_str()).map(str::to_string),
            };
            let mut patch = Vec::new();
            for mut op in started.patch {
                // The attribution's place waits where the person gave none: never filled in for them.
                if unsaid.is_some() && [op.get("id"), op.get("node")].into_iter().flatten().any(|v| v.as_str() == unsaid.as_deref()) {
                    continue;
                }
                if slotted(&op, POEM) {
                    op["node"]["text"] = json!(poem);
                } else if let Some(words) = attribution
                    && slotted(&op, ATTRIBUTION)
                {
                    op["node"]["text"] = json!(words);
                }
                patch.push(op);
            }
            (patch, started.id, poem_id)
        }
        Place::Slot { slot } => {
            let mut filled = crate::states::filling(deck, theme, state, slot)?;
            filled.patch[0]["node"]["text"] = json!(poem);
            (filled.patch, state.to_string(), filled.id)
        }
        Place::Over { node } => {
            let snaps = scaena_core::resolve_states(deck).context("tracking")?;
            let shown = (snaps.iter().find(|s| s.state_id == state))
                .and_then(|s| s.nodes.get(node))
                .ok_or_else(|| OpsError::new(format!("{node} is not on screen in {state}")))?;
            let before = scaena_core::reading::words(shown);
            let patch = (replacing(&before, &poem).into_iter())
                .map(|r| json!({ "op": "replace_text", "state": state, "node": node, "from": r.from, "to": r.to, "text": r.text }))
                .collect();
            (patch, state.to_string(), node.clone())
        }
    };
    // The credit, after what the notes say already.
    let was = deck.states.iter().find(|s| s.id == on).and_then(|s| s.notes.clone()).unwrap_or_default();
    let notes = if was.trim().is_empty() { credit.to_string() } else { format!("{}\n\n{credit}", was.trim_end()) };
    patch.push(json!({ "op": "set_state", "id": on, "prop": "notes", "value": notes }));
    Ok(Placed { patch, state: on, node })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poem(text: &str) -> Poem {
        Poem::counted(haiku::taken(text, "en"), "en")
    }

    #[test]
    fn a_near_miss_is_asked_once_more_and_the_closer_offered_with_a_note() {
        let about = About::Topic { topic: "a pond".into() };
        let kept = "An old silent pond\nA frog jumps into the pond\nSplash! Silence again";
        let near = "An old silent pond\nA green frog jumps into the pond\nSplash! Silence again";
        let far = "Pond\nA green frog jumps into the pond again and again\nSplash";
        // Kept, or uncounted: offered at once.
        assert!(matches!(answered(&about, "en", poem(kept), None), Answered::Offer { note: None, .. }));
        assert!(matches!(answered(&about, "en", poem("in 1999\na\nb"), None), Answered::Offer { note: None, .. }));
        // Off: asked once more, with each line's count.
        let Answered::Again { prompt } = answered(&about, "en", poem(near), None) else { panic!("asked again") };
        assert!(prompt.contains("Its lines count 5, 8, and 5 syllables"), "{prompt}");
        // The second kept: offered.
        let Answered::Offer { poem: p, note } = answered(&about, "en", poem(near), Some(poem(kept))) else { panic!() };
        assert_eq!((p.lines[1].as_str(), note), ("A frog jumps into the pond", None));
        // Neither: the closer, with a note; the second on a tie.
        let Answered::Offer { poem: p, note } = answered(&about, "en", poem(near), Some(poem(far))) else { panic!() };
        assert_eq!(p.lines[1], "A green frog jumps into the pond");
        assert_eq!(note.as_deref(), Some("Asked twice, the closest counts 5, 8, and 5 syllables, not 5, 7, and 5."));
        let tie = "An old silent pond\nA frog jumps into the pond\nSplash! Silence once again";
        let Answered::Offer { poem: p, .. } = answered(&about, "en", poem(near), Some(poem(tie))) else { panic!() };
        assert_eq!(p.lines[2], "Splash! Silence once again", "the second on a tie");
    }

    #[test]
    fn a_japanese_poem_counts_by_its_reading() {
        let taken = haiku::taken("古池や\n蛙飛び込む\n水の音\n\nふるいけや\nかわずとびこむ\nみずのおと", "ja");
        let p = Poem::counted(taken, "ja");
        assert_eq!(p.checked.kept, Kept::Kept);
        assert_eq!(p.checked.lines[0].text, "古池や");
    }
}
