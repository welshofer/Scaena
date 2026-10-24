//! What the model is told before the user's first word (SPEC §11): what Scaena is, what its
//! tools do here, the deck it works on, what it can read, and the author-deck skill; and what
//! the client shows as a question is asked, which begins it. For the browser's editor
//! ([`Client::Page`]) the words are `web/src/assistant/prompt.ts`'s, which the recorded
//! exchanges hold them to; the Mac's window ([`Client::App`]) differs only where the two clients
//! differ.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who the conversation is for: the browser's editor, or the Mac app's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Client {
    Page,
    #[default]
    App,
}

impl Client {
    /// What the user works in, as the prompt names it.
    fn place(self) -> &'static str {
        match self {
            Client::Page => "editor",
            Client::App => "window",
        }
    }
}

/// The open deck, in a few facts: its title, states, formats, and theme; and its story, where it
/// has one (narrative FUNC-2): the brief it was made from, the arc it was written to, and each
/// beat's claim in spine order, so what the assistant does is aimed at the story the user asked
/// for, not at lint alone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub states: Vec<String>,
    #[serde(default)]
    pub formats: Vec<String>,
    /// The theme's name: its file's, without `.theme.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// The brief the deck was made from (`meta.narrative.brief`), where it was made from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<Brief>,
    /// The arc it was written to (`meta.narrative.arc`), where it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<String>,
    /// Each beat's claim, in spine order, those left blank passed over.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<String>,
}

/// The brief a deck was made from, as `meta.narrative.brief` keeps it (ADR-0026): what it is
/// about, who it is for, what it is to do, how long it was asked to run, how it is delivered,
/// what the person wants it to achieve, and the voice its words take (narrative FUNC-4).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modality: Option<String>,
    #[serde(default, rename = "desiredOutcomes", skip_serializing_if = "Option::is_none")]
    pub desired_outcomes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
}

impl Facts {
    /// The facts of `deck` (as `deck_read` returns it), with its states and formats.
    pub fn of(deck: &Value, states: Vec<String>, formats: Vec<String>) -> Facts {
        let title = deck.pointer("/meta/title").and_then(Value::as_str).map(str::to_string);
        let theme = deck["theme"].as_str().map(|path| {
            let file = path.rsplit('/').next().unwrap_or(path);
            file.strip_suffix(".theme.json").unwrap_or(file).to_string()
        });
        let text = |at: &str| deck.pointer(at).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        let brief = deck.pointer("/meta/narrative/brief").map(|_| Brief {
            topic: text("/meta/narrative/brief/topic").map(str::to_string),
            audience: text("/meta/narrative/brief/audience").map(str::to_string),
            goal: text("/meta/narrative/brief/goal").map(str::to_string),
            length: text("/meta/narrative/brief/length").map(str::to_string),
            modality: text("/meta/narrative/brief/modality").map(str::to_string),
            desired_outcomes: text("/meta/narrative/brief/desiredOutcomes").map(str::to_string),
            voice: text("/meta/narrative/brief/voice").map(str::to_string),
        });
        let brief = brief.filter(|b| *b != Brief::default());
        let arc = text("/meta/narrative/arc").map(str::to_string);
        let claims = (deck.pointer("/spine/sections").and_then(Value::as_array).into_iter().flatten())
            .flat_map(|section| section["beats"].as_array().into_iter().flatten())
            .filter_map(|beat| beat["claim"].as_str().map(str::trim).filter(|c| !c.is_empty()))
            .map(str::to_string)
            .collect();
        Facts { title, states, formats, theme, brief, arc, claims }
    }

    /// The deck's story, as the prompt tells it: nothing where the deck has none.
    fn story(&self) -> String {
        let mut about = Vec::new();
        if let Some(brief) = &self.brief {
            if let Some(topic) = &brief.topic {
                about.push(format!("on {}", Value::from(topic.as_str())));
            }
            if let Some(audience) = &brief.audience {
                about.push(format!("for {audience}"));
            }
            if let Some(goal) = &brief.goal {
                about.push(format!("to {goal}"));
            }
            if let Some(length) = &brief.length {
                about.push(format!("in {length}"));
            }
        }
        if let Some(arc) = &self.arc {
            about.push(format!("written to the {arc} arc"));
        }
        // How it is delivered, what it is to achieve, and its voice, each a sentence of its own.
        let said: Vec<String> = (self.brief.iter())
            .flat_map(|b| {
                [("Delivered", &b.modality), ("What it is to achieve", &b.desired_outcomes), ("Its voice", &b.voice)]
            })
            .filter_map(|(label, value)| value.as_deref().map(|v| format!("{label}: {}.", v.trim_end_matches('.'))))
            .collect();
        if about.is_empty() && said.is_empty() && self.claims.is_empty() {
            return String::new();
        }
        let mut story = String::from("\n\nIts story");
        if !about.is_empty() {
            story.push_str(": ");
            story.push_str(&about.join(", "));
        }
        if !said.is_empty() {
            story.push('.');
            for sentence in &said {
                story.push(' ');
                story.push_str(sentence);
            }
            if !self.claims.is_empty() {
                story.push_str(" Its beats' claims, in order:");
                for (i, claim) in self.claims.iter().enumerate() {
                    story.push_str(&format!("\n{}. {claim}", i + 1));
                }
            }
        } else if self.claims.is_empty() {
            story.push('.');
        } else {
            story.push_str(if about.is_empty() {
                ", by its beats' claims, in order:"
            } else {
                ". Its beats' claims, in order:"
            });
            for (i, claim) in self.claims.iter().enumerate() {
                story.push_str(&format!("\n{}. {claim}", i + 1));
            }
        }
        story
    }
}

/// What the client shows as a question is asked (PLAN 2.52): the state shown, in a format if not
/// the deck's own, the nodes selected, and the characters selected in a text typed in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Seeing {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    pub nodes: Vec<Selected>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub characters: Option<Characters>,
}

/// A node selected, and its type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selected {
    pub node: String,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Characters selected in a text, from `from` to `to` in Unicode scalar values, as
/// `replace_text` and `style_text` count them, with the text they make.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Characters {
    pub node: String,
    pub from: u64,
    pub to: u64,
    pub text: String,
}

/// What the client shows as a question is asked, as the question's first line, in brackets: the
/// system prompt says how to read it. The conversation keeps it with the question, so what was
/// selected then stays said. Nothing where the client says nothing.
pub fn seen(client: Client, seeing: Option<&Seeing>) -> String {
    let Some(seeing) = seeing else { return String::new() };
    let format = seeing.format.as_ref().map(|f| format!(" in {f}")).unwrap_or_default();
    let nodes = if seeing.nodes.is_empty() {
        "nothing selected".to_string()
    } else {
        let named: Vec<String> = (seeing.nodes.iter())
            .map(|n| match &n.kind {
                Some(kind) => format!("{} ({kind})", n.node),
                None => n.node.clone(),
            })
            .collect();
        format!("selected: {}", named.join(", "))
    };
    let characters = seeing
        .characters
        .as_ref()
        .map(|c| {
            format!("; in {}, characters {} to {} selected: {}", c.node, c.from, c.to, Value::from(c.text.as_str()))
        })
        .unwrap_or_default();
    format!("[In the {}: state {} shown{format}; {nodes}{characters}.]\n\n", client.place(), seeing.state)
}

/// A resource the model is told of, as `resources/list` names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    pub uri: String,
    pub name: String,
}

/// The system prompt for the deck `facts` tells of, in `client`, the resources the MCP server
/// lists named and the author-deck skill in full; with `bundle_skills`, the skills the bundle
/// carries (`skills/NAME/SKILL.md`), by name.
pub fn system(client: Client, facts: &Facts, bundle_skills: &[String]) -> String {
    let listed: Vec<Listed> = (scaena_resources::all().iter().filter(|r| r.listed))
        .map(|r| Listed { uri: r.uri.clone(), name: r.name.clone() })
        .collect();
    let author_deck = scaena_resources::resource("scaena://skills/author-deck").unwrap_or_default();
    system_of(client, facts, &listed, bundle_skills, author_deck)
}

/// The system prompt, from what it names: [`system`] with the resources and the author-deck
/// skill given.
pub fn system_of(
    client: Client,
    facts: &Facts,
    resources: &[Listed],
    bundle_skills: &[String],
    author_deck: &str,
) -> String {
    let skills = if bundle_skills.is_empty() {
        String::new()
    } else {
        let lines: Vec<String> = bundle_skills.iter().map(|s| format!("- bundle://skills/{s}")).collect();
        format!(
            "\nThe bundle carries its own skills too; follow them over the general ones where they differ:\n{}\n",
            lines.join("\n")
        )
    };
    let place = client.place();
    let (app, dropped, bundles, after) = match client {
        Client::Page => (
            "editor",
            "A file the user dropped on the editor is in the bundle, data_attach",
            "the page opens and downloads bundles itself, and its Versions tab reads and restores the deck's history",
            " What you change is selected in the editor after.",
        ),
        Client::App => {
            ("Mac app", "data_attach", "the app opens and saves bundles itself, and macOS keeps their versions", "")
        }
    };
    let title = facts.title.as_ref().map(|t| format!("\"{t}\", ")).unwrap_or_default();
    let n = facts.states.len();
    let plural = if n == 1 { "" } else { "s" };
    let theme = facts.theme.as_ref().map(|t| format!(", theme {t}")).unwrap_or_default();
    let formats =
        if facts.formats.is_empty() { String::new() } else { format!(", also in {}", facts.formats.join(", ")) };
    let resources: Vec<String> = resources.iter().map(|r| format!("- {}: {}", r.uri, r.name)).collect();
    format!(
        "You are the assistant in Scaena's {app}, working with the user on the deck they have open.

Scaena decks are states over one scene graph: nodes exist for the whole deck, each state says what changes, and the theme owns type and layout, so a deck names roles, slots, and presets, never pixels.

Your tools are the Scaena MCP server's, working on the open deck: they take no `bundle`, `out`, or `painter`. Each edit you make (deck_patch, spine_update, state_start, background_set, slide_write, data_attach, data_edit, theme_edit, deck_lint with `fix`) shows in the user's {place} as you make it, and they can undo it. Edit with deck_patch (`dry_run` first when unsure), check with deck_lint, read the story with deck_health, which scores how the slides pace, balance, and follow their arc and says what to look at on each, and look with deck_render, which returns the frame as an image. slide_write writes one slide as the deck's generator builds them, from a slide in the JSON Narrato writes, after a slide or in a slide's place: ask it for a new slide, or for one written again from scratch, rather than building one node by node. {dropped} declares a data file the bundle holds, and data_edit reads a data source's rows and edits them in place: a cell set, a row added or taken away. theme_edit edits the deck's own theme, its colors, type roles, and spacing, by JSON Patch operations on it (resource_read of bundle://theme reads it as it is, and scaena://schema/theme says what it takes): a look asked of the whole deck, such as larger headlines or a warmer accent, is an edit of the theme, never a literal written into each node. There is no deck_create, deck_generate, theme_apply, deck_export, or deck_history here: {bundles}, so ask the user when the work needs a new bundle, another theme, an export, or an earlier version.

resource_read reads the resources: the schemas, the lint catalog, the specification by section (scaena://spec is its index), the skills, and examples. The patch ops are in scaena://schema/patch. Follow the author-deck skill below, and read the others when the work calls for them.

A question may begin with what the user sees in the {place}, in brackets: the state shown, the nodes selected, and any characters selected in a text, counted as replace_text and style_text count them, with the text they make. \"This\", \"it\", \"these\", \"here\", and words like \"shorter\" or \"bolder\" mean what it names: change those, in that state, unless the question says otherwise.{after}

Say briefly what you do and what you found, in the user's language. Lint to zero errors before you say a deck is done, and look at what you changed. Before you say a deck is done, read its health with deck_health and leave each slide saying its beat's claim.

The deck: {title}{n} state{plural} ({states}){theme}{formats}.{story}

The resources:
{resources}
{skills}
--- scaena://skills/author-deck ---
{author_deck}",
        states = facts.states.join(", "),
        story = facts.story(),
        resources = resources.join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_decks_facts_name_its_theme_by_its_file() {
        let deck = json!({ "meta": { "title": "Q3" }, "theme": "themes/dusk.theme.json" });
        let facts = Facts::of(&deck, vec!["cover".into()], vec![]);
        assert_eq!(facts.title.as_deref(), Some("Q3"));
        assert_eq!(facts.theme.as_deref(), Some("dusk"));
        assert_eq!(Facts::of(&json!({ "theme": { "inline": true } }), vec![], vec![]).theme, None);
        assert_eq!((facts.brief, facts.arc, facts.claims), (None, None, vec![]), "a deck with no story tells none");
    }

    /// A deck made from a brief tells the assistant its story (narrative FUNC-2): the brief, the
    /// arc, and each beat's claim in spine order, so its edits aim at the story, not at lint alone.
    #[test]
    fn a_decks_facts_tell_its_story() {
        let deck = json!({
            "meta": { "title": "Ridgeline", "narrative": {
                "arc": "Hero's Journey", "reasoning": "…",
                "brief": { "topic": "Fifty-two miles of trail", "audience": "Donors", "goal": "inspire", "length": "5-10 slides" },
            } },
            "spine": { "sections": [
                { "id": "a", "beats": [{ "id": "b1", "claim": "The trail was lost." }, { "id": "b2", "claim": "  " }] },
                { "id": "b", "beats": [{ "id": "b3", "claim": "Volunteers rebuilt it." }] },
            ] },
        });
        let facts = Facts::of(&deck, vec!["cover".into()], vec![]);
        assert_eq!(facts.arc.as_deref(), Some("Hero's Journey"));
        assert_eq!(facts.brief.as_ref().and_then(|b| b.goal.as_deref()), Some("inspire"));
        assert_eq!(facts.claims, ["The trail was lost.", "Volunteers rebuilt it."], "a blank claim is passed over");
        let system = system_of(Client::App, &facts, &[], &[], "skill");
        assert!(
            system.contains(
                "The deck: \"Ridgeline\", 1 state (cover).\n\nIts story: on \"Fifty-two miles of trail\", for Donors, to inspire, in 5-10 slides, written to the Hero's Journey arc. Its beats' claims, in order:\n1. The trail was lost.\n2. Volunteers rebuilt it.\n\nThe resources:"
            ),
            "{system}"
        );
        assert!(system.contains("read its health with deck_health and leave each slide saying its beat's claim."));

        // A brief with nothing in it tells nothing; claims alone are told by themselves.
        let bare = Facts::of(&json!({ "meta": { "narrative": { "brief": {} } } }), vec![], vec![]);
        assert_eq!(bare.brief, None);
        assert_eq!(bare.story(), "");
        let claims = Facts { claims: vec!["One.".into()], ..Facts::default() };
        assert_eq!(claims.story(), "\n\nIts story, by its beats' claims, in order:\n1. One.");
        let arc = Facts { arc: Some("Sparkline".into()), ..Facts::default() };
        assert_eq!(arc.story(), "\n\nIts story: written to the Sparkline arc.");

        // The whole brief is told (narrative FUNC-4): how it is delivered, what it is to achieve,
        // and its voice, each a sentence of its own, before the claims.
        let mut whole = deck.clone();
        whole["meta"]["narrative"]["brief"]["modality"] = json!("In-person \u{2014} presenter in the room");
        whole["meta"]["narrative"]["brief"]["desiredOutcomes"] = json!("Pledges for the next season.");
        whole["meta"]["narrative"]["brief"]["voice"] = json!("Warm, plain, no jargon");
        let facts = Facts::of(&whole, vec!["cover".into()], vec![]);
        assert_eq!(
            facts.story(),
            "\n\nIts story: on \"Fifty-two miles of trail\", for Donors, to inspire, in 5-10 slides, written to the Hero's Journey arc. Delivered: In-person \u{2014} presenter in the room. What it is to achieve: Pledges for the next season. Its voice: Warm, plain, no jargon. Its beats' claims, in order:\n1. The trail was lost.\n2. Volunteers rebuilt it."
        );
        let voice = Facts { brief: Some(Brief { voice: Some("Dry".into()), ..Brief::default() }), ..Facts::default() };
        assert_eq!(voice.story(), "\n\nIts story. Its voice: Dry.");
    }

    #[test]
    fn the_app_says_window_where_the_page_says_editor() {
        let facts = Facts { states: vec!["one".into()], ..Facts::default() };
        let app = system_of(Client::App, &facts, &[], &[], "skill");
        assert!(app.starts_with("You are the assistant in Scaena's Mac app,"));
        assert!(!app.contains("editor"), "{app}");
        assert!(app.contains("shows in the user's window as you make it"));
        assert!(app.contains("what the user sees in the window, in brackets"));
        assert!(app.contains("\n\nThe deck: 1 state (one).\n\n"));
        let seeing = Seeing { state: "one".into(), ..Seeing::default() };
        assert_eq!(seen(Client::App, Some(&seeing)), "[In the window: state one shown; nothing selected.]\n\n");
    }
}
