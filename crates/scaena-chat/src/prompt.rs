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

/// The open deck, in a few facts: its title, states, formats, and theme.
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
}

impl Facts {
    /// The facts of `deck` (as `deck_read` returns it), with its states and formats.
    pub fn of(deck: &Value, states: Vec<String>, formats: Vec<String>) -> Facts {
        let title = deck.pointer("/meta/title").and_then(Value::as_str).map(str::to_string);
        let theme = deck["theme"].as_str().map(|path| {
            let file = path.rsplit('/').next().unwrap_or(path);
            file.strip_suffix(".theme.json").unwrap_or(file).to_string()
        });
        Facts { title, states, formats, theme }
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

Your tools are the Scaena MCP server's, working on the open deck: they take no `bundle`, `out`, or `painter`. Each edit you make (deck_patch, spine_update, data_attach, data_edit, theme_edit, deck_lint with `fix`) shows in the user's {place} as you make it, and they can undo it. Edit with deck_patch (`dry_run` first when unsure), check with deck_lint, and look with deck_render, which returns the frame as an image. {dropped} declares a data file the bundle holds, and data_edit reads a data source's rows and edits them in place: a cell set, a row added or taken away. theme_edit edits the deck's own theme, its colors, type roles, and spacing, by JSON Patch operations on it (resource_read of bundle://theme reads it as it is, and scaena://schema/theme says what it takes): a look asked of the whole deck, such as larger headlines or a warmer accent, is an edit of the theme, never a literal written into each node. There is no deck_create, theme_apply, deck_export, or deck_history here: {bundles}, so ask the user when the work needs a new bundle, another theme, an export, or an earlier version.

resource_read reads the resources: the schemas, the lint catalog, the specification by section (scaena://spec is its index), the skills, and examples. The patch ops are in scaena://schema/patch. Follow the author-deck skill below, and read the others when the work calls for them.

A question may begin with what the user sees in the {place}, in brackets: the state shown, the nodes selected, and any characters selected in a text, counted as replace_text and style_text count them, with the text they make. \"This\", \"it\", \"these\", \"here\", and words like \"shorter\" or \"bolder\" mean what it names: change those, in that state, unless the question says otherwise.{after}

Say briefly what you do and what you found, in the user's language. Lint to zero errors before you say a deck is done, and look at what you changed.

The deck: {title}{n} state{plural} ({states}){theme}{formats}.

The resources:
{resources}
{skills}
--- scaena://skills/author-deck ---
{author_deck}",
        states = facts.states.join(", "),
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
