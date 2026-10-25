//! What a model is asked to write for the person editing (PLAN 2.101, `scaena_core::writing`): a
//! text's words proofread, tightened, or rewritten, a slide's notes drafted, a finding explained,
//! a picture described (PLAN 2.119), a slide's words checked against its beat's claim and its
//! headline rewritten to carry it (narrative FUNC-4). The session says what to ask, from the deck as it stands,
//! with the picture a description is of, and the patch that takes an answer: the browser's page
//! and the Mac each make the call to the model themselves, on the device or with the person's own
//! key, and offer the answer to take or leave.

use crate::{Error, Session};
use base64::Engine as _;
use scaena_core::document::{NodeType, Props};
use scaena_core::writing::{Explained, Task, checked, replacing};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The most pixels a side of the picture a model is shown to describe it (PLAN 2.119): enough to
/// say what it shows, a few hundred tokens of each provider's.
pub const SEEN: u32 = 768;

/// What a client asks to have written: the task's `kind`, the state shown, and the text node it
/// is about, or the finding to explain. A finding is explained without a state where none holds
/// it, as a source's that does not compile: its words are then not looked up.
#[derive(Debug, Clone, Deserialize)]
pub struct Ask {
    pub kind: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub finding: Option<Finding>,
    /// For a rewrite, how the person wants it rewritten, in their own words ("more formal",
    /// "shorter, for executives"; lift-up FUNC-3).
    #[serde(default)]
    pub instruction: Option<String>,
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
/// prompt it answers; for a description, the picture it is shown beside the prompt, as the slide
/// shows it, a PNG in base64.
#[derive(Debug, Clone, Serialize)]
pub struct Writing {
    pub task: Task,
    pub title: &'static str,
    pub instructions: &'static str,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
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
        let picture = match (&task, ask.node.as_deref()) {
            (Task::Describe { .. }, Some(node)) => Some(self.seen(&ask.state, node)?),
            _ => None,
        };
        Ok(Writing { title: task.title(), instructions: task.instructions(), prompt: task.prompt(), task, picture })
    }

    /// `answer` as it is offered for `ask`, and the patch that takes it, made on the deck as it
    /// stands: the changes that make the text read as the answer, each a `replace_text` written
    /// where the text lives, the words it keeps keeping their runs; or the state's notes; or,
    /// for a claim check, the headline the answer rewrites, set in the slide's claim node the
    /// same way, nothing where the words carry the claim. One patch, one step to undo. No
    /// answer, as a model that refuses or runs out of room gives, changes nothing: it empties no
    /// text and no notes.
    pub fn written(&mut self, ask: &Ask, answer: &str) -> Result<Written, Error> {
        let task = self.task(ask)?;
        let text = task.taken(answer);
        let none = text.trim().is_empty();
        let patch = match &task {
            // The headline rewritten to carry the claim, where the answer offers one that reads
            // otherwise; nothing where the words carry it, or no verdict can be read.
            Task::Claim { headline, .. } => {
                let offered = checked(&text).and_then(|c| c.headline);
                let node = self.claim_node(&ask.state)?;
                Some(match (offered, node, headline) {
                    (Some(after), Some(id), Some(before)) if after.trim() != before.trim() => {
                        (replacing(before, &after).into_iter())
                            .map(|r| {
                                json!({ "op": "replace_text", "state": ask.state, "node": id, "from": r.from, "to": r.to, "text": r.text })
                            })
                            .collect()
                    }
                    _ => Vec::new(),
                })
            }
            Task::Proofread { .. } | Task::Tighten { .. } | Task::Rewrite { .. } | Task::Notes { .. } if none => {
                Some(Vec::new())
            }
            Task::Proofread { text: before }
            | Task::Tighten { text: before, .. }
            | Task::Rewrite { text: before, .. } => {
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
            // The picture's description, written where it lives; nothing where the answer is
            // none, or the description it has.
            Task::Describe { text: was, .. } => {
                let node = ask.node.as_deref().unwrap_or_default();
                let same = none || was.as_deref() == Some(text.as_str());
                Some(if same {
                    Vec::new()
                } else {
                    vec![json!({ "op": "choose", "state": ask.state, "node": node, "prop": "alt", "value": text })]
                })
            }
        };
        Ok(Written { text, patch })
    }

    /// The node that makes `state`'s claim, whose words a claim check may rewrite (narrative
    /// FUNC-4): the text it shows with `semantic: claim`, else its first heading as it reads;
    /// none where it shows neither.
    fn claim_node(&mut self, state: &str) -> Result<Option<String>, Error> {
        let i = self.deck.state_index(state).ok_or_else(|| Error::Ops(format!("no state `{state}`")))?;
        let snapshots = scaena_core::resolve_states(&self.deck).map_err(|e| Error::Deck(e.to_string()))?;
        let claims = (snapshots[i].nodes.iter())
            .filter(|(id, props)| {
                self.deck.nodes.get(*id).map(|n| n.node_type) == Some(NodeType::Text)
                    && props.get("semantic").and_then(Value::as_str) == Some("claim")
            })
            .map(|(id, _)| id.clone())
            .next();
        if claims.is_some() {
            return Ok(claims);
        }
        let parts = self.reads(state)?;
        Ok(parts.iter().find(|p| p.role == "heading" && !p.text.trim().is_empty()).map(|p| p.node.clone()))
    }

    /// The claim of the beat whose states hold `state`, where the spine gives one.
    fn claim_of(&self, state: &str) -> Option<String> {
        (self.deck().spine.as_ref())
            .and_then(|s| s.sections.iter().flat_map(|s| &s.beats).find(|b| b.states.iter().any(|s| s == state)))
            .map(|b| b.claim.clone())
            .filter(|c| !c.trim().is_empty())
    }

    /// The deck's voice, as its brief says it (`meta.narrative.brief.voice`, narrative FUNC-4).
    fn voice(&self) -> Option<String> {
        let meta = self.deck().meta.as_ref()?;
        let voice = meta.extra.get("narrative")?.pointer("/brief/voice")?.as_str()?.trim();
        (!voice.is_empty()).then(|| voice.to_string())
    }

    /// `node`'s props as `state` shows them, its overrides over them.
    fn props_shown(&self, state: &str, node: &str) -> Result<Props, Error> {
        let i = self.deck.state_index(state).ok_or_else(|| Error::Ops(format!("no state `{state}`")))?;
        let snapshots = scaena_core::resolve_states(&self.deck).map_err(|e| Error::Deck(e.to_string()))?;
        let mut shown = (snapshots[i].nodes.get(node).cloned())
            .ok_or_else(|| Error::Ops(format!("{node} is not on screen in {state}")))?;
        if let Some(over) = self.deck.overrides.get(node) {
            scaena_core::tracking::merge_props(&mut shown, over);
        }
        Ok(shown)
    }

    /// The picture `node` shows in `state`, as a model is shown it to describe (PLAN 2.119): the
    /// image's pixels cropped and adjusted as the slide shows them, made no more than [`SEEN`]
    /// pixels a side, a PNG in base64.
    fn seen(&mut self, state: &str, node: &str) -> Result<String, Error> {
        let shown = self.props_shown(state, node)?;
        let src = (shown.get("src").and_then(Value::as_str))
            .ok_or_else(|| Error::Ops(format!("{node} shows no picture to describe")))?
            .to_string();
        let bytes = self.file(&src).ok_or_else(|| Error::Missing(src.clone()))?;
        let id = scaena_engine::images::ImageInfo::read(bytes).map_err(|e| Error::Ops(format!("{src}: {e}")))?.id;
        self.build()?;
        let picture = self.store.image(&id)?;
        let (mut rgba, mut width, mut height) = (picture.rgba.data().to_vec(), picture.width, picture.height);
        if let Some([x, y, w, h]) = shown.get("crop").and_then(|c| serde_json::from_value::<[f64; 4]>(c.clone()).ok()) {
            (rgba, width, height) = cropped(&rgba, width, height, [x, y, w, h]);
        }
        if let Some(Value::Object(a)) = shown.get("adjust") {
            let fraction = |key: &str| a.get(key).and_then(Value::as_f64).unwrap_or(0.0).clamp(-1.0, 1.0);
            let adjust = scaena_core::adjust::Adjust::from_fractions(
                fraction("exposure"),
                fraction("contrast"),
                fraction("saturation"),
                fraction("warmth"),
            );
            adjust.apply(&mut rgba);
        }
        let (rgba, width, height) = smaller(&rgba, width, height, SEEN);
        let png = scaena_paint::Raster { width, height, rgba }.to_png_fast()?;
        Ok(base64::engine::general_purpose::STANDARD.encode(png))
    }

    fn task(&mut self, ask: &Ask) -> Result<Task, Error> {
        let words = |s: &mut Session, node: &str| -> Result<Option<String>, Error> {
            Ok(s.carets(&ask.state, node)?.map(|c| c.text))
        };
        Ok(match ask.kind.as_str() {
            "proofread" | "tighten" | "rewrite" => {
                let node = ask.node.as_deref().ok_or_else(|| Error::Ops(format!("{}: `node` is a text", ask.kind)))?;
                // Code is written as it runs, not as prose reads (PLAN 2.139).
                if self.carets(&ask.state, node)?.is_some_and(|c| c.code.is_some()) {
                    return Err(Error::Ops(format!(
                        "{}: `{node}` is code, written as it runs: its words are not proofread, tightened, or rewritten",
                        ask.kind
                    )));
                }
                let text = (words(self, node)?)
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(|| Error::Ops("the text has no words to write: type some first".into()))?;
                // The point the slide's beat makes and the deck's voice, which a tightening and a
                // rewrite keep to, and the person's own words for how to rewrite it (lift-up FUNC-3).
                let (claim, voice) = (self.claim_of(&ask.state), self.voice());
                match ask.kind.as_str() {
                    "proofread" => Task::Proofread { text },
                    "tighten" => Task::Tighten { text, claim, voice },
                    _ => {
                        let ask =
                            ask.instruction.as_deref().map(str::trim).filter(|a| !a.is_empty()).map(str::to_string);
                        Task::Rewrite { text, ask, claim, voice }
                    }
                }
            }
            "notes" => {
                let slide = scaena_core::writing::slide(&self.reads(&ask.state)?);
                Task::Notes { slide, claim: self.claim_of(&ask.state) }
            }
            "explain" => {
                let f =
                    ask.finding.as_ref().ok_or_else(|| Error::Ops("explain: `finding` is what it explains".into()))?;
                // The words of what it is about, as the state it holds in shows them.
                let words = match f.node.as_deref() {
                    Some(node) if !ask.state.is_empty() => words(self, node).unwrap_or_default(),
                    _ => None,
                };
                Task::Explain(Explained {
                    code: f.code.clone(),
                    severity: f.severity.clone(),
                    message: f.said.clone().unwrap_or_else(|| f.message.clone()),
                    hint: f.hint.clone(),
                    words,
                })
            }
            "describe" => {
                let node = ask
                    .node
                    .as_deref()
                    .ok_or_else(|| Error::Ops("describe: `node` is the picture it describes".into()))?;
                if self.deck.nodes.get(node).map(|n| n.node_type) != Some(NodeType::Image) {
                    return Err(Error::Ops(format!("{node} is not a picture: Describe describes one")));
                }
                let text = (self.props_shown(&ask.state, node)?.get("alt").and_then(Value::as_str))
                    .filter(|t| !t.trim().is_empty())
                    .map(str::to_string);
                // The slide's words but the picture's own, which its description is.
                let parts: Vec<_> = self.reads(&ask.state)?.into_iter().filter(|p| p.node != node).collect();
                Task::Describe { slide: scaena_core::writing::slide(&parts), text }
            }
            // Whether the slide's words carry the claim its beat makes (narrative FUNC-4): the
            // claim from the spine, the slide as it reads, and its headline, the claim node's words.
            "claim" => {
                let claim = (self.deck().spine.as_ref())
                    .and_then(|s| s.sections.iter().flat_map(|s| &s.beats).find(|b| b.states.contains(&ask.state)))
                    .map(|b| b.claim.clone())
                    .filter(|c| !c.trim().is_empty())
                    .ok_or_else(|| {
                        Error::Ops("the slide is in no beat of the story, so it makes no claim to check".into())
                    })?;
                let slide = scaena_core::writing::slide(&self.reads(&ask.state)?);
                let headline = match self.claim_node(&ask.state)? {
                    Some(node) => words(self, &node)?.filter(|t| !t.trim().is_empty()),
                    None => None,
                };
                Task::Claim { claim, slide, headline }
            }
            other => {
                return Err(Error::Ops(format!(
                    "`{other}` is no task: proofread, tighten, rewrite, notes, explain, describe, or claim"
                )));
            }
        })
    }
}

/// The part `[x, y, w, h]` of a picture `width` by `height`, in fractions of it, as its own
/// picture: at least a pixel a side.
fn cropped(rgba: &[u8], width: u32, height: u32, [x, y, w, h]: [f64; 4]) -> (Vec<u8>, u32, u32) {
    let along = |from: f64, span: f64, side: u32| {
        let first = ((from.clamp(0.0, 1.0) * f64::from(side)).round() as u32).min(side - 1);
        let last = (((from + span).clamp(0.0, 1.0) * f64::from(side)).round() as u32).clamp(first + 1, side);
        (first, last - first)
    };
    let ((x0, w), (y0, h)) = (along(x, w, width), along(y, h, height));
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for row in y0..y0 + h {
        let start = (row as usize * width as usize + x0 as usize) * 4;
        out.extend_from_slice(&rgba[start..start + w as usize * 4]);
    }
    (out, w, h)
}

/// A picture `width` by `height` made no more than `side` pixels a side, its shape kept: each
/// pixel the mean of those it covers, weighted by their alpha, so what is clear lends no color.
/// One no larger is as it was.
fn smaller(rgba: &[u8], width: u32, height: u32, side: u32) -> (Vec<u8>, u32, u32) {
    let longest = width.max(height);
    if longest <= side {
        return (rgba.to_vec(), width, height);
    }
    let scaled =
        |n: u32| ((u64::from(n) * u64::from(side) + u64::from(longest) / 2) / u64::from(longest)).max(1) as u32;
    let (w, h) = (scaled(width), scaled(height));
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    let span = |i: u32, n: u32, of: u32| {
        let first = (u64::from(i) * u64::from(of) / u64::from(n)) as u32;
        let last = ((u64::from(i + 1) * u64::from(of)).div_ceil(u64::from(n)) as u32).clamp(first + 1, of);
        first..last
    };
    for oy in 0..h {
        let rows = span(oy, h, height);
        for ox in 0..w {
            let columns = span(ox, w, width);
            let (mut sum, mut alpha, mut count) = ([0u64; 3], 0u64, 0u64);
            for y in rows.clone() {
                for x in columns.clone() {
                    let p = (y as usize * width as usize + x as usize) * 4;
                    let a = u64::from(rgba[p + 3]);
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u64::from(rgba[p + c]) * a;
                    }
                    alpha += a;
                    count += 1;
                }
            }
            let channel = |s: u64| (s + alpha / 2).checked_div(alpha).map_or(0, |v| v as u8);
            out.extend_from_slice(&[
                channel(sum[0]),
                channel(sum[1]),
                channel(sum[2]),
                ((alpha + count / 2) / count) as u8,
            ]);
        }
    }
    (out, w, h)
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

    fn torture() -> Session {
        const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
        let read = |p: &str| std::fs::read_to_string(format!("{BUNDLE}/{p}")).unwrap();
        let mut s = Session::new(&read("deck.json"), &read("theme.json")).unwrap();
        let parsed = Deck::from_json(&read("deck.json")).unwrap();
        for font in &parsed.fonts {
            s.add_file(&font.file, std::fs::read(format!("{BUNDLE}/{}", font.file)).unwrap());
        }
        for path in ["data/bars.csv", "data/bars-next.csv"] {
            s.add_file(path, std::fs::read(format!("{BUNDLE}/{path}")).unwrap());
        }
        for path in s.image_files() {
            s.add_file(&path, std::fs::read(format!("{BUNDLE}/{path}")).unwrap());
        }
        s
    }

    /// The picture a model is shown, as Describe hands it over.
    fn seen(w: &Writing) -> scaena_paint::Raster {
        let png = base64::engine::general_purpose::STANDARD.decode(w.picture.as_deref().unwrap()).unwrap();
        scaena_paint::Raster::from_png(&png).unwrap()
    }

    fn ask(kind: &str, state: &str, node: Option<&str>) -> Ask {
        Ask { kind: kind.into(), state: state.into(), node: node.map(str::to_string), finding: None, instruction: None }
    }

    /// The revenue example, its subtitle set by `runs`.
    fn revenue_runs(runs: Value) -> Session {
        revenue_with(|deck| {
            let subtitle = deck["nodes"]["subtitle"].as_object_mut().unwrap();
            subtitle.shift_remove("text");
            subtitle.insert("runs".into(), runs);
        })
    }

    /// The revenue example, changed by `edit`.
    fn revenue_with(edit: impl FnOnce(&mut Value)) -> Session {
        let dir = "../../docs/examples";
        let mut deck: Value =
            serde_json::from_str(&std::fs::read_to_string(format!("{dir}/revenue.deck.json")).unwrap()).unwrap();
        edit(&mut deck);
        let deck = deck.to_string();
        let mut s =
            Session::new(&deck, &std::fs::read_to_string(format!("{dir}/themes/dusk.theme.json")).unwrap()).unwrap();
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

    /// `answer` to `ask` taken, as Use it takes it; the subtitle's runs after.
    fn taken(s: &mut Session, ask: &Ask, answer: &str) -> Value {
        let patch = s.written(ask, answer).unwrap().patch.unwrap();
        assert!(s.typed(&Value::Array(patch), None).unwrap());
        s.deck().nodes["subtitle"].props["runs"].clone()
    }

    /// A word put right keeps its look: the change starts where the emphasized word does, and
    /// the corrected word is still emphasized, not set in the run before it; a link still covers
    /// its word. A clause in a script written without spaces keeps the runs of the characters
    /// the answer keeps.
    #[test]
    fn a_word_put_right_keeps_the_look_of_its_run() {
        let proofread = ask("proofread", "intro", Some("subtitle"));
        let mut s = revenue_runs(json!([
            { "text": "This quarter " }, { "text": "chnaged", "emphasis": "high" }, { "text": " the shape of the business." }
        ]));
        assert_eq!(
            taken(&mut s, &proofread, "This quarter changed the shape of the business."),
            json!([{ "text": "This quarter " }, { "text": "changed", "emphasis": "high" }, { "text": " the shape of the business." }])
        );
        let link = json!({ "href": "https://example.com" });
        let mut s = revenue_runs(
            json!([{ "text": "Sign up on " }, { "text": "teh", "link": link }, { "text": " portal today." }]),
        );
        assert_eq!(
            taken(&mut s, &proofread, "Sign up on the portal today."),
            json!([{ "text": "Sign up on " }, { "text": "the", "link": link }, { "text": " portal today." }])
        );
        let mut s = revenue_runs(json!([
            { "text": "売上は" }, { "text": "前年比", "emphasis": "high" }, { "text": "で増加した。" }
        ]));
        assert_eq!(
            taken(&mut s, &proofread, "売上は前年比で増加しました。"),
            json!([{ "text": "売上は" }, { "text": "前年比", "emphasis": "high" }, { "text": "で増加しました。" }])
        );
    }

    /// A rewrite is asked with the person's own words for how, the claim of the slide's beat, and
    /// the deck's voice from its brief; a tightening with the claim and the voice; a proofreading
    /// with neither (lift-up FUNC-3). Its answer is taken as any rewrite's is.
    #[test]
    fn a_rewrite_is_asked_with_the_persons_words_the_claim_and_the_voice() {
        let mut s =
            revenue_with(|deck| deck["meta"]["narrative"] = json!({ "brief": { "voice": "Plain, confident" } }));
        let mut rewrite = ask("rewrite", "intro", Some("subtitle"));
        rewrite.instruction = Some("  More formal, for the board ".into());
        let w = s.writing(&rewrite).unwrap();
        assert_eq!(
            w.prompt,
            "The text:\nThis quarter changed the shape of the business.\nHow to rewrite it: More formal, for the board\nThe point its slide makes: This quarter changed the shape of the business.\nThe deck's voice: Plain, confident"
        );
        assert_eq!(serde_json::to_value(&w.task).unwrap()["ask"], "More formal, for the board");
        let tighten = s.writing(&ask("tighten", "intro", Some("subtitle"))).unwrap();
        assert!(tighten.prompt.ends_with("\nThe deck's voice: Plain, confident") && !tighten.prompt.contains("How to"));
        let proofread = s.writing(&ask("proofread", "intro", Some("subtitle"))).unwrap();
        assert_eq!(proofread.prompt, "This quarter changed the shape of the business.");
        let taken = s.written(&rewrite, "This quarter reshaped the business.").unwrap();
        assert_eq!(taken.patch.unwrap().len(), 1);
    }

    /// No answer from the model, or one of nothing but spaces, changes nothing: it deletes no
    /// text's words and empties no notes.
    #[test]
    fn no_answer_changes_nothing() {
        let mut s = revenue();
        for kind in ["proofread", "tighten", "rewrite"] {
            let written = s.written(&ask(kind, "intro", Some("subtitle")), "  \n").unwrap();
            assert_eq!(written.patch, Some(vec![]), "{kind}");
        }
        assert_eq!(s.written(&ask("notes", "intro", None), "").unwrap().patch, Some(vec![]));
    }

    /// A finding no state holds, as a source that does not compile gives, is explained all the
    /// same: an explanation needs no state but to find the words of what it is about.
    #[test]
    fn a_finding_of_no_state_is_explained() {
        let mut s = revenue();
        let explain: Ask = serde_json::from_value(json!({
            "kind": "explain",
            "finding": { "code": "E106", "severity": "error", "message": "line 3: expected a value", "said": "The source has a mistake." }
        }))
        .unwrap();
        assert_eq!(s.writing(&explain).unwrap().prompt, "Finding E106 (error): The source has a mistake.");
    }

    /// A text's words as the state shows them, asked in the engine's words; an answer taken by the
    /// least `replace_text`s, written where the text lives, the quotes a model wraps it in off.
    #[test]
    fn a_text_is_asked_as_it_reads_and_its_answer_taken_word_by_word() {
        let mut s = revenue();
        let tighten = ask("tighten", "intro", Some("subtitle"));
        let w = s.writing(&tighten).unwrap();
        assert_eq!(w.title, "Tightened");
        // Asked with the point its slide makes, which here its own words say (lift-up FUNC-3).
        let claim = "\nThe point its slide makes: This quarter changed the shape of the business.";
        assert_eq!(w.prompt, format!("The text:\nThis quarter changed the shape of the business.{claim}"));
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
        assert_eq!(
            s.writing(&tighten).unwrap().prompt,
            format!("The text:\nThis quarter reshaped the business.{claim}")
        );
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
            instruction: None,
        };
        let w = s.writing(&explain).unwrap();
        assert_eq!(w.prompt, "Finding E100 (error): The words run out of their box.\nIts words: Q3 Review");
        assert!(s.written(&explain, "It overflows.").unwrap().patch.is_none(), "an explanation is only read");
        assert!(s.writing(&ask("polish", "intro", None)).is_err());
    }

    /// A slide's words are checked against the claim its beat makes (narrative FUNC-4): the
    /// model is asked with the claim, the slide as it reads, and its headline, the claim node's
    /// words; a headline it rewrites is taken as `replace_text`s on that node, the words it keeps
    /// keeping their runs; words that carry the claim, or an answer with no verdict, take
    /// nothing; and a slide in no beat has no claim to check.
    #[test]
    fn a_slides_words_are_checked_against_its_beats_claim() {
        let mut s = revenue();
        let claim = ask("claim", "revenue", None);
        let w = s.writing(&claim).unwrap();
        assert_eq!(w.title, "The Claim");
        assert_eq!(w.task.text(), Some("Revenue doubled"));
        assert_eq!(
            w.prompt,
            "The claim its beat makes: Revenue doubled year over year, and Pro drove it.\nThe slide reads:\nTitle: Revenue doubled\nPicture: Quarterly revenue by product, Q4 2025 through Q3 2026.\nText: Revenue in $M. Enterprise recognized on delivery.\nIts headline: Revenue doubled"
        );
        let drifted = "Drifts: the slide says revenue doubled, not that Pro drove it.\nHeadline: Revenue doubled, and Pro drove it";
        let written = s.written(&claim, drifted).unwrap();
        assert_eq!(written.text, drifted);
        let patch = written.patch.unwrap();
        assert_eq!(
            patch,
            vec![
                json!({ "op": "replace_text", "state": "revenue", "node": "title", "from": 15, "to": 15, "text": ", and Pro drove it" })
            ]
        );
        assert!(s.typed(&Value::Array(patch), None).unwrap());
        assert_eq!(s.carets("revenue", "title").unwrap().unwrap().text, "Revenue doubled, and Pro drove it");
        // Words that carry the claim, the same headline, or no verdict take nothing.
        for answer in
            ["Carries.", "Drifts: a little.\nHeadline: Revenue doubled, and Pro drove it", "I cannot say.", ""]
        {
            assert!(s.written(&claim, answer).unwrap().patch.unwrap().is_empty(), "{answer:?}");
        }
        // The cover's claim is its subtitle, the text with `semantic: claim`.
        let intro = s.writing(&ask("claim", "intro", None)).unwrap();
        assert_eq!(intro.task.text(), Some("This quarter changed the shape of the business."));
        assert!(intro.prompt.starts_with("The claim its beat makes: This quarter changed the shape of the business."));
        // A slide in no beat has no claim to check.
        let mut s = revenue_with(|deck| {
            for section in deck["spine"]["sections"].as_array_mut().unwrap() {
                for beat in section["beats"].as_array_mut().unwrap() {
                    if beat["id"] == "doubled" {
                        beat["states"] = json!(["revenue"]);
                    }
                }
            }
        });
        let e = s.writing(&ask("claim", "mix", None)).unwrap_err();
        assert!(e.to_string().contains("no beat of the story"), "{e}");
    }

    /// A picture is described from itself as the slide shows it, and the slide's other words
    /// (PLAN 2.119): the model is shown its pixels adjusted and cropped as they are drawn, and
    /// its answer is taken as the picture's description, one `choose` written where it lives.
    #[test]
    fn a_picture_is_described_as_the_slide_shows_it() {
        let mut s = torture();
        let mono = ask("describe", "adjust", Some("adj-mono"));
        let w = s.writing(&mono).unwrap();
        assert_eq!(w.title, "Description");
        assert!(w.instructions.starts_with("You describe a picture"));
        assert!(w.prompt.starts_with("Describe this picture. The slide it is on reads:\n"), "{}", w.prompt);
        assert!(w.prompt.contains("Adjust Image"), "the slide's words: {}", w.prompt);
        assert!(!w.prompt.contains("in black and white"), "but not the picture's own description: {}", w.prompt);
        assert_eq!(w.task.text(), Some("The same picture in black and white."));
        // Its saturation is -1: the model sees it in black and white, at its own size.
        let picture = seen(&w);
        assert_eq!((picture.width, picture.height), (67, 45));
        let gray = |p: &[u8]| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
        assert!(picture.rgba.chunks(4).all(|p| gray(p) <= 2), "black and white");
        let plain = seen(&s.writing(&ask("describe", "adjust", Some("adj-plain"))).unwrap());
        assert!(plain.rgba.chunks(4).any(|p| gray(p) > 60), "where the plain photo has color");
        // A crop is seen as the slide shows it: an eighth of the test card a side.
        let card = s.file("assets/test-card.png").map(|b| scaena_paint::Picture::decode(b).unwrap()).unwrap();
        let zoom = seen(&s.writing(&ask("describe", "images", Some("image-zoom"))).unwrap());
        assert_eq!((zoom.width, zoom.height), (card.width / 8, card.height / 8));

        // The answer is the picture's description: one `choose`, its quotes off.
        let taken = s.written(&mono, "“A gray gradient, a dark block, and a lighter disc.”").unwrap();
        assert_eq!(taken.text, "A gray gradient, a dark block, and a lighter disc.");
        let patch = taken.patch.unwrap();
        assert_eq!(
            patch,
            vec![json!({ "op": "choose", "state": "adjust", "node": "adj-mono", "prop": "alt",
                "value": "A gray gradient, a dark block, and a lighter disc." })]
        );
        assert!(s.typed(&Value::Array(patch), None).unwrap());
        assert_eq!(s.writing(&mono).unwrap().task.text(), Some("A gray gradient, a dark block, and a lighter disc."));
        let again = s.written(&mono, "A gray gradient, a dark block, and a lighter disc.").unwrap();
        assert_eq!(again.patch, Some(vec![]), "the description it has already changes nothing");
        assert_eq!(s.written(&mono, "  ").unwrap().patch, Some(vec![]), "nor does no answer");

        // Only a picture is described.
        let e = s.writing(&ask("describe", "adjust", Some("case"))).unwrap_err().to_string();
        assert!(e.contains("not a picture"), "{e}");
        assert!(s.writing(&ask("describe", "adjust", None)).is_err());
    }

    /// A picture past what a model is shown is made smaller, its shape kept, each pixel the mean
    /// of those it covers; what is clear lends it no color. A part is cut by fractions.
    #[test]
    fn a_picture_is_made_smaller_and_cut_as_it_is_shown() {
        // 1600 × 800: red on the left half, clear blue on the right.
        let (w, h) = (1600u32, 800u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(if x < w / 2 { &[200, 20, 10, 255] } else { &[0, 0, 255, 0] });
            }
        }
        let (small, sw, sh) = smaller(&rgba, w, h, SEEN);
        assert_eq!((sw, sh), (768, 384));
        assert_eq!(&small[..4], &[200, 20, 10, 255]);
        let last = small.len() - 4;
        assert_eq!(small[last + 3], 0, "the clear half stays clear");
        // At the seam, the clear half lends no blue.
        let seam = (383 * 4) as usize;
        assert!(small[seam + 2] < 20 && small[seam + 3] > 100, "{:?}", &small[seam..seam + 4]);
        assert_eq!(smaller(&rgba[..64], 4, 4, SEEN), (rgba[..64].to_vec(), 4, 4), "one no larger is as it was");

        let (part, pw, ph) = cropped(&rgba, w, h, [0.25, 0.5, 0.5, 0.25]);
        assert_eq!((pw, ph), (800, 200));
        assert_eq!(&part[..4], &[200, 20, 10, 255]);
        assert_eq!(part[part.len() - 1], 0, "its right half is the clear blue");
        assert_eq!(cropped(&rgba, w, h, [1.0, 1.0, 0.0, 0.0]).1, 1, "at least a pixel a side");
    }
}
