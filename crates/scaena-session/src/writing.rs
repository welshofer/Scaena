//! What a model is asked to write for the person editing (PLAN 2.101, `scaena_core::writing`): a
//! text's words proofread, tightened, or rewritten, a slide's notes drafted, a finding explained.
//! The session says what to ask, from the deck as it stands, and the patch that takes an answer:
//! the browser's page and the Mac each make the call to the model themselves, on the device or
//! with the person's own key, and offer the answer to take or leave.

use crate::{Error, Session};
use scaena_core::writing::{Explained, Task, replacing};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a client asks to have written: the task's `kind`, the state shown, and the text node it
/// is about, or the finding to explain.
#[derive(Debug, Clone, Deserialize)]
pub struct Ask {
    pub kind: String,
    pub state: String,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub finding: Option<Finding>,
}

/// A finding to explain, as the window shows it (`said`) and lint wrote it.
#[derive(Debug, Clone, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub said: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub node: Option<String>,
}

/// What to ask a model: the task, what it is called where it is offered, and the instructions and
/// prompt it answers.
#[derive(Debug, Clone, Serialize)]
pub struct Writing {
    pub task: Task,
    pub title: &'static str,
    pub instructions: &'static str,
    pub prompt: String,
}

/// An answer as it is offered, and the patch that takes it: none for one that is only read.
#[derive(Debug, Clone, Serialize)]
pub struct Written {
    pub text: String,
    pub patch: Option<Vec<Value>>,
}

impl Session {
    /// What to ask a model for `ask`, from the deck as it stands: a text's words as `state` shows
    /// them, the slide's parts as they read with the claim of its beat, or the finding with the
    /// words of what it is about.
    pub fn writing(&mut self, ask: &Ask) -> Result<Writing, Error> {
        let task = self.task(ask)?;
        Ok(Writing { title: task.title(), instructions: task.instructions(), prompt: task.prompt(), task })
    }

    /// `answer` as it is offered for `ask`, and the patch that takes it, made on the deck as it
    /// stands: the changes that make the text read as the answer, each a `replace_text` written
    /// where the text lives, the words it keeps keeping their runs; or the state's notes. One
    /// patch, one step to undo.
    pub fn written(&mut self, ask: &Ask, answer: &str) -> Result<Written, Error> {
        let task = self.task(ask)?;
        let text = task.taken(answer);
        let patch = match &task {
            Task::Proofread { text: before } | Task::Tighten { text: before } | Task::Rewrite { text: before } => {
                let node = ask.node.as_deref().unwrap_or_default();
                Some(
                    (replacing(before, &text).into_iter())
                        .map(|r| {
                            json!({ "op": "replace_text", "state": ask.state, "node": node, "from": r.from, "to": r.to, "text": r.text })
                        })
                        .collect(),
                )
            }
            Task::Notes { .. } => {
                Some(vec![json!({ "op": "set_state", "id": ask.state, "prop": "notes", "value": text })])
            }
            Task::Explain(_) => None,
        };
        Ok(Written { text, patch })
    }

    fn task(&mut self, ask: &Ask) -> Result<Task, Error> {
        let words = |s: &mut Session, node: &str| -> Result<Option<String>, Error> {
            Ok(s.carets(&ask.state, node)?.map(|c| c.text))
        };
        Ok(match ask.kind.as_str() {
            "proofread" | "tighten" | "rewrite" => {
                let node = ask.node.as_deref().ok_or_else(|| Error::Ops(format!("{}: `node` is a text", ask.kind)))?;
                let text = (words(self, node)?)
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(|| Error::Ops(format!("{node} has no words in {} to write", ask.state)))?;
                match ask.kind.as_str() {
                    "proofread" => Task::Proofread { text },
                    "tighten" => Task::Tighten { text },
                    _ => Task::Rewrite { text },
                }
            }
            "notes" => {
                let slide = scaena_core::writing::slide(&self.reads(&ask.state)?);
                let claim = (self.deck().spine.as_ref())
                    .and_then(|s| s.sections.iter().flat_map(|s| &s.beats).find(|b| b.states.contains(&ask.state)))
                    .map(|b| b.claim.clone());
                Task::Notes { slide, claim }
            }
            "explain" => {
                let f =
                    ask.finding.as_ref().ok_or_else(|| Error::Ops("explain: `finding` is what it explains".into()))?;
                let words = match f.node.as_deref() {
                    Some(node) => words(self, node).unwrap_or_default(),
                    None => None,
                };
                Task::Explain(Explained {
                    code: f.code.clone(),
                    severity: f.severity.clone(),
                    message: f.said.clone().unwrap_or_else(|| f.message.clone()),
                    hint: f.hint.clone(),
                    words,
                })
            }
            other => {
                return Err(Error::Ops(format!(
                    "`{other}` is no task: proofread, tighten, rewrite, notes, or explain"
                )));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_core::Deck;

    fn revenue() -> Session {
        let dir = "../../docs/examples";
        let read = |p: &str| std::fs::read_to_string(format!("{dir}/{p}")).unwrap();
        let deck = read("revenue.deck.json");
        let mut s = Session::new(&deck, &read("themes/dusk.theme.json")).unwrap();
        let parsed = Deck::from_json(&deck).unwrap();
        for font in &parsed.fonts {
            s.add_file(&font.file, std::fs::read(format!("{dir}/{}", font.file)).unwrap());
        }
        for source in parsed.data.values() {
            if let Value::String(path) = &source.source {
                s.add_file(path, std::fs::read(format!("{dir}/{path}")).unwrap());
            }
        }
        s
    }

    fn ask(kind: &str, state: &str, node: Option<&str>) -> Ask {
        Ask { kind: kind.into(), state: state.into(), node: node.map(str::to_string), finding: None }
    }

    /// A text's words as the state shows them, asked in the engine's words; an answer taken by the
    /// least `replace_text`s, written where the text lives, the quotes a model wraps it in off.
    #[test]
    fn a_text_is_asked_as_it_reads_and_its_answer_taken_word_by_word() {
        let mut s = revenue();
        let tighten = ask("tighten", "intro", Some("subtitle"));
        let w = s.writing(&tighten).unwrap();
        assert_eq!(w.title, "Tightened");
        assert_eq!(w.prompt, "This quarter changed the shape of the business.");
        assert!(w.instructions.starts_with("You tighten the words of one text on a slide"));
        let taken = s.written(&tighten, "“This quarter reshaped the business.”").unwrap();
        assert_eq!(taken.text, "This quarter reshaped the business.");
        let patch = taken.patch.unwrap();
        assert_eq!(
            patch,
            vec![
                json!({ "op": "replace_text", "state": "intro", "node": "subtitle", "from": 13, "to": 33, "text": "reshaped" })
            ]
        );
        // The patch makes the text read as the answer.
        let ops = Value::Array(patch);
        s.typed(&ops, None).unwrap();
        assert_eq!(s.writing(&tighten).unwrap().prompt, "This quarter reshaped the business.");
        assert!(s.writing(&ask("tighten", "intro", Some("bg"))).is_err(), "a shader has no words to write");
    }

    /// A slide's notes are drafted from its parts as they read and the claim of its beat, and taken
    /// as its own notes; a finding is explained with the words of what it is about, and only read.
    #[test]
    fn notes_are_drafted_from_the_slide_and_a_finding_explained_from_its_words() {
        let mut s = revenue();
        let notes = ask("notes", "intro", None);
        let w = s.writing(&notes).unwrap();
        assert_eq!(
            w.prompt,
            "The slide reads:\nTitle: Q3 Review\nHeading: This quarter changed the shape of the business.\nThe point it makes: This quarter changed the shape of the business."
        );
        let drafted = s.written(&notes, "Open on the quarter.\n").unwrap();
        assert_eq!(
            drafted.patch.unwrap(),
            vec![json!({ "op": "set_state", "id": "intro", "prop": "notes", "value": "Open on the quarter." })]
        );

        let explain = Ask {
            kind: "explain".into(),
            state: "intro".into(),
            node: None,
            finding: Some(Finding {
                code: "E100".into(),
                severity: "error".into(),
                message: "text `title` overflows".into(),
                said: Some("The words run out of their box.".into()),
                hint: None,
                node: Some("title".into()),
            }),
        };
        let w = s.writing(&explain).unwrap();
        assert_eq!(w.prompt, "Finding E100 (error): The words run out of their box.\nIts words: Q3 Review");
        assert!(s.written(&explain, "It overflows.").unwrap().patch.is_none(), "an explanation is only read");
        assert!(s.writing(&ask("polish", "intro", None)).is_err());
    }
}
