//! A haiku for the person editing (PLAN O.4d, ADR-0030 decisions 26–29): what their model is
//! asked, about a topic, the slide shown, or the whole talk; what an answer comes to, asked once
//! more where it is off 5-7-5; and the patch that places the poem offered, one step to undo. The
//! browser's page and the Mac each make the call to the model themselves, as the writing tools'
//! are made (`writing`), and offer the poem to edit, write again, take, or leave.

use crate::{Error, Session};
use scaena_core::verse::{self, Checked};
use scaena_core::writing::haiku::{self, About};
use scaena_ops::haiku::{Answered, Place, Placed, Poem, Writer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a haiku is asked about, from the state shown: `topic` with what was typed, `slide`, or
/// `talk`; and, for Write Another, the poem not to write again.
#[derive(Debug, Clone, Deserialize)]
pub struct HaikuAsk {
    pub state: String,
    pub about: String,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub another: Option<Vec<String>>,
}

/// What to ask the model: the instructions and the prompt, the language it is asked in, and what
/// it is about, as the notes will say it.
#[derive(Debug, Clone, Serialize)]
pub struct HaikuWriting {
    pub instructions: String,
    pub prompt: String,
    pub lang: String,
    pub about: String,
}

/// An answer handed back, or lines the person edited, and where the poem goes when taken.
#[derive(Debug, Clone, Deserialize)]
pub struct HaikuTake {
    pub ask: HaikuAsk,
    /// The model's answer to the ask, or to the second ask where `first` is given.
    #[serde(default)]
    pub answer: Option<String>,
    /// The poem the first ask came to, where this answer is the second's.
    #[serde(default)]
    pub first: Option<Poem>,
    /// Lines as the person left them in the dialog: offered as they are, asked nothing more.
    #[serde(default)]
    pub lines: Option<Vec<String>>,
    /// Where it goes: `new`, a Haiku slide after the slide shown; `slide`, this slide. None asks
    /// for no patch yet.
    #[serde(default)]
    pub to: Option<String>,
    /// The verse text selected, which a poem placed on this slide takes the place of.
    #[serde(default)]
    pub node: Option<String>,
    /// Where the pointer last pressed, canvas units: where a poem placed on this slide goes when
    /// the slide has no empty verse place and no verse is selected.
    #[serde(default)]
    pub at: Option<[f32; 2]>,
    #[serde(default)]
    pub attribution: Option<String>,
    /// Who wrote it, and the client's date, for the notes.
    #[serde(default)]
    pub by: Option<Writer>,
    #[serde(default)]
    pub date: Option<String>,
}

/// What an answer comes to: the poem to offer and its note, or the second ask; with the patch that
/// places the poem where it was asked to go.
#[derive(Debug, Clone, Serialize)]
pub struct HaikuTaken {
    #[serde(flatten)]
    pub answered: Answered,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placed: Option<Placed>,
}

impl Session {
    /// The deck's language, as a haiku is asked in it: its `meta.lang`, else English.
    fn language(&self) -> String {
        self.deck.meta.as_ref().and_then(|m| m.lang.clone()).unwrap_or_else(|| "en".to_string())
    }

    /// What `ask` is about, from the deck as it stands.
    fn about(&mut self, ask: &HaikuAsk) -> Result<About, Error> {
        Ok(match ask.about.as_str() {
            "topic" => {
                let topic = ask.topic.as_deref().map(str::trim).unwrap_or_default();
                if topic.is_empty() {
                    return Err(Error::Ops("say what the haiku is about".into()));
                }
                About::Topic { topic: topic.to_string() }
            }
            "slide" => {
                let slide = scaena_core::writing::slide(&self.reads(&ask.state)?);
                let claim = (self.deck.spine.as_ref())
                    .and_then(|s| s.sections.iter().flat_map(|s| &s.beats).find(|b| b.states.contains(&ask.state)))
                    .map(|b| b.claim.clone());
                About::Slide { slide, claim }
            }
            "talk" => About::Talk { talk: scaena_ops::haiku::talk_of(&self.deck) },
            other => {
                return Err(Error::Ops(format!("a haiku is about a topic, this slide, or the talk, not `{other}`")));
            }
        })
    }

    /// What to ask a model for `ask`, in the deck's language: refused before any call where the
    /// theme's fonts cannot draw it.
    pub fn haiku(&mut self, ask: &HaikuAsk) -> Result<HaikuWriting, Error> {
        let about = self.about(ask)?;
        let lang = self.language();
        // A language in a script the theme's fonts do not draw is refused before any call.
        if let Some(why) = scaena_ops::haiku::undrawable(&self.theme, &lang) {
            return Err(Error::Ops(why));
        }
        let prompt = match &ask.another {
            Some(poem) if !poem.is_empty() => haiku::another(&about, poem),
            _ => haiku::prompt(&about),
        };
        Ok(HaikuWriting { instructions: haiku::instructions(&lang), prompt, lang, about: about.said() })
    }

    /// What `take` comes to: the first answer offered where it keeps to 5-7-5 or cannot be
    /// counted, else the second ask; the second answer offered where it keeps, else the closer of
    /// the two with a note; lines the person edited offered as they are. Where `to` says where it
    /// goes, the patch that places it.
    pub fn haiku_taken(&mut self, take: &HaikuTake) -> Result<HaikuTaken, Error> {
        let about = self.about(&take.ask)?;
        let lang = self.language();
        let answered = match (&take.lines, &take.answer) {
            (Some(lines), _) => {
                let poem = Poem::counted(haiku::Taken { lines: lines.clone(), reading: None }, &lang);
                Answered::Offer { poem, note: None }
            }
            (None, Some(answer)) => {
                let poem = Poem::counted(haiku::taken(answer, &lang), &lang);
                if poem.lines.is_empty() {
                    return Err(Error::Ops("the model wrote no haiku: ask it again".into()));
                }
                match &take.first {
                    Some(first) => scaena_ops::haiku::answered(&about, &lang, first.clone(), Some(poem)),
                    None => scaena_ops::haiku::answered(&about, &lang, poem, None),
                }
            }
            (None, None) => return Err(Error::Ops("a haiku is taken from an answer or from its lines".into())),
        };
        let placed = match (&answered, take.to.as_deref()) {
            (Answered::Offer { poem, .. }, Some(to)) => Some(self.haiku_placing(take, &about, &poem.lines, to)?),
            _ => None,
        };
        Ok(HaikuTaken { answered, placed })
    }

    /// The patch that places `lines` as `to` says, from the state asked about.
    fn haiku_placing(&mut self, take: &HaikuTake, about: &About, lines: &[String], to: &str) -> Result<Placed, Error> {
        let state = take.ask.state.as_str();
        let by = take.by.clone().unwrap_or(Writer { model: "your model".into(), provider: None });
        let credit = scaena_ops::haiku::credited(&by, take.date.as_deref().unwrap_or("today"), about);
        let place = match to {
            "new" => Place::New,
            "slide" => match self.haiku_place(state, take.node.as_deref())? {
                Some(place) => place,
                None => return self.haiku_inserting(state, lines, take.at.unwrap_or_default(), &credit),
            },
            other => return Err(Error::Ops(format!("a haiku goes on a new slide or this one, not `{other}`"))),
        };
        let placed = scaena_ops::haiku::placing(
            &self.deck,
            &self.theme,
            state,
            lines,
            take.attribution.as_deref(),
            &place,
            &credit,
        )
        .map_err(|e| Error::Ops(e.to_string()))?;
        Ok(placed)
    }

    /// Where a poem placed on `state`'s slide goes, if not where the pointer pressed: the slide's
    /// empty verse place, else the verse text selected.
    fn haiku_place(&mut self, state: &str, node: Option<&str>) -> Result<Option<Place>, Error> {
        let waiting = self.waiting(state)?;
        if let Some(slot) = waiting.iter().find(|w| w.typed && w.role.as_deref() == Some("verse")) {
            return Ok(Some(Place::Slot { slot: slot.slot.clone() }));
        }
        if let Some(node) = node {
            let shown = self.props_shown(state, node)?;
            let role = shown.get("role").and_then(Value::as_str);
            if role == Some("verse") || shown.get("wrap").and_then(Value::as_str) == Some("none") {
                return Ok(Some(Place::Over { node: node.to_string() }));
            }
        }
        Ok(None)
    }

    /// A poem inserted where the pointer pressed, kept to its slide, as Insert puts a text: in the
    /// theme's `verse` role, else `quote`, else `lede`, set as written.
    fn haiku_inserting(&mut self, state: &str, lines: &[String], at: [f32; 2], credit: &str) -> Result<Placed, Error> {
        let inserts = self.inserts();
        let role = ["verse", "quote", "lede"]
            .into_iter()
            .find(|r| inserts.iter().any(|i| i.node["type"] == "text" && i.id == *r))
            .ok_or_else(|| Error::Ops("the theme has no role to set a haiku in".into()))?;
        let n = inserts.iter().position(|i| i.node["type"] == "text" && i.id == role).unwrap_or_default();
        let mut with = serde_json::Map::new();
        with.insert("text".into(), Value::String(lines.join("\n")));
        if role != "verse" {
            with.insert("wrap".into(), Value::String("none".into()));
        }
        let added = self.inserting_with(state, n, at, Some("haiku"), Some(&with))?;
        let mut patch = added.patch;
        let was = self.deck.states.iter().find(|s| s.id == state).and_then(|s| s.notes.clone()).unwrap_or_default();
        let notes = if was.trim().is_empty() { credit.to_string() } else { format!("{}\n\n{credit}", was.trim_end()) };
        patch.push(serde_json::json!({ "op": "set_state", "id": state, "prop": "notes", "value": notes }));
        Ok(Placed { patch, state: state.to_string(), node: added.id })
    }

    /// Each line's count as the deck's language counts it, and how the poem keeps to 5-7-5: what
    /// the dialog shows beside each line as it is edited.
    pub fn verse_counts(&self, lines: &[String]) -> Checked {
        verse::check(&lines.join("\n"), &self.language())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn revenue() -> Session {
        let dir = "../../docs/examples";
        let read = |p: &str| std::fs::read_to_string(format!("{dir}/{p}")).unwrap();
        let mut s = Session::new(&read("revenue.deck.json"), &read("themes/dusk.theme.json")).unwrap();
        for path in [
            "fonts/Fraunces-VF.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "fonts/Fraunces-Italic-VF.ttf",
            "fonts/Inter-Italic-VF.ttf",
            "fonts/JetBrainsMono-Italic-VF.ttf",
            "data/q3-revenue.csv",
        ] {
            s.add_file(path, std::fs::read(format!("{dir}/{path}")).unwrap());
        }
        s
    }

    fn ask(about: &str) -> HaikuAsk {
        HaikuAsk { state: "revenue".into(), about: about.into(), topic: Some("the quarter".into()), another: None }
    }

    fn take(ask: HaikuAsk) -> HaikuTake {
        HaikuTake {
            ask,
            answer: None,
            first: None,
            lines: None,
            to: None,
            node: None,
            at: None,
            attribution: None,
            by: Some(Writer { model: "test-model-1".into(), provider: Some("Anthropic".into()) }),
            date: Some("October 15, 2026".into()),
        }
    }

    const NEAR: &str = "An old silent pond\nA green frog jumps into the pond\nSplash! Silence again";
    const KEPT: &str = "An old silent pond\nA frog jumps into the pond\nSplash! Silence again";

    #[test]
    fn a_haiku_is_asked_about_a_topic_the_slide_or_the_talk() {
        let mut s = revenue();
        let topic = s.haiku(&ask("topic")).unwrap();
        assert_eq!(topic.prompt, "Write a haiku about: the quarter");
        assert!(topic.instructions.contains("in English"));
        let slide = s.haiku(&ask("slide")).unwrap();
        assert!(slide.prompt.starts_with("Write a haiku about this slide. It reads:\n"), "{}", slide.prompt);
        let talk = s.haiku(&ask("talk")).unwrap();
        assert!(talk.prompt.contains("<talk>\n") && talk.about == "the whole talk", "{}", talk.prompt);
        let another = HaikuAsk { another: Some(vec!["a".into(), "b".into(), "c".into()]), ..ask("topic") };
        assert!(s.haiku(&another).unwrap().prompt.ends_with("Write a different haiku from this one:\na\nb\nc"));
        let none = HaikuAsk { topic: None, ..ask("topic") };
        assert!(s.haiku(&none).is_err());
        // A deck in Japanese, in a theme with no font for it, is refused before any call.
        let mut ja = s.deck.clone();
        ja.meta.get_or_insert_default().lang = Some("ja".into());
        s.set_deck(ja);
        let refused = s.haiku(&ask("topic")).unwrap_err().to_string();
        assert!(refused.contains("no font for Japanese"), "{refused}");
    }

    #[test]
    fn a_near_miss_is_asked_once_more_then_placed_on_a_new_slide_in_one_patch() {
        let mut s = revenue();
        let first = s.haiku_taken(&HaikuTake { answer: Some(NEAR.into()), ..take(ask("topic")) }).unwrap();
        let Answered::Again { prompt } = &first.answered else { panic!("asked again: {first:?}") };
        assert!(prompt.contains("Its lines count 5, 8, and 5 syllables"));
        assert!(first.placed.is_none());
        // The second keeps to 5-7-5: offered, and placed where it was asked to go.
        let near = Poem::counted(haiku::taken(NEAR, "en"), "en");
        let second = s
            .haiku_taken(&HaikuTake {
                answer: Some(KEPT.into()),
                first: Some(near),
                to: Some("new".into()),
                ..take(ask("topic"))
            })
            .unwrap();
        let Answered::Offer { poem, note } = &second.answered else { panic!("offered") };
        assert_eq!((poem.lines[1].as_str(), note), ("A frog jumps into the pond", &None));
        let placed = second.placed.unwrap();
        let states = s.deck.states.len();
        s.tool("deck_patch", json!({ "ops": placed.patch }), crate::assistant::Caller { author: "user", at: None })
            .unwrap();
        assert_eq!(s.deck.states.len(), states + 1, "one slide");
        let slide = s.deck.states.iter().find(|st| st.id == placed.state).unwrap();
        assert_eq!(slide.layout.as_deref(), Some("haiku"));
        assert_eq!(
            slide.notes.as_deref(),
            Some("Haiku written by test-model-1 with your Anthropic key on October 15, 2026, about the quarter.")
        );
        assert!(!serde_json::to_string(&s.deck.nodes).unwrap().contains("test-model-1"));
        // Lines the person edited are offered as they are, and counted as they read.
        let edited = s
            .haiku_taken(&HaikuTake {
                lines: Some(vec!["one two".into(), "three".into(), "four".into()]),
                ..take(ask("topic"))
            })
            .unwrap();
        let Answered::Offer { poem, .. } = edited.answered else { panic!() };
        assert_eq!(poem.checked.kept, verse::Kept::Off);
        let counts = s.verse_counts(&["An old silent pond".into(), "A frog".into()]);
        assert_eq!(counts.lines.iter().map(|l| l.count.unwrap().low).collect::<Vec<_>>(), [5, 2]);
    }
}
