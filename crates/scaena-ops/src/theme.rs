//! Re-theme a deck (PLAN 1.6, SPEC §3.6): point it at another theme, copied into the bundle,
//! and say what that changes in what `validate` and `lint` find.

use crate::lint::{View, errors, lint, lint_in, write_deck};
use crate::{Bundle, Context, OpsError};
use scaena_core::Finding;
use scaena_core::lint::{Delta, delta};
use scaena_core::validate::BundleFiles;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// What a re-theme did, or would do.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Themed {
    /// The theme's path in the bundle.
    pub theme: String,
    /// The theme the deck named before: a path, `(inline)`, or none.
    pub was: Option<String>,
    /// Whether the deck and the theme were written: not under a dry run.
    pub applied: bool,
    /// Families set in the bundle's font of that family, the theme's own file not being in
    /// the bundle: `family `key`: theirs → ours`.
    pub mapped: Vec<String>,
    /// What `validate` and `lint` find with the new theme that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not with it.
    pub removed: Vec<Finding>,
    /// The findings that are errors, with the new theme.
    pub errors: usize,
}

/// Point the bundle's deck at the theme file `theme`, copying it to `themes/` unless it is
/// in the bundle already. A family whose file the bundle does not hold is set in the bundle
/// font of that family, if it has one: a saved bundle names fonts by their content. The
/// deck is not otherwise touched, and is written canonically.
pub fn theme_apply(b: &Bundle, theme: &Path, dry_run: bool) -> Result<Themed, OpsError> {
    let mut text = std::fs::read_to_string(theme).with_context(|| format!("reading {}", theme.display()))?;
    let mut parsed: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", theme.display()))?;
    let mut mapped = Vec::new();
    for (key, family) in parsed.pointer_mut("/type/families").and_then(|f| f.as_object_mut()).into_iter().flatten() {
        let (Some(file), Some(name)) = (family["file"].as_str(), family["family"].as_str()) else { continue };
        if b.files.exists(file) {
            continue;
        }
        if let Some(font) = b.deck.fonts.iter().find(|f| f.family == name && b.files.exists(&f.file)) {
            mapped.push(format!("family `{key}`: {file} → {}", font.file));
            family["file"] = serde_json::Value::String(font.file.clone());
        }
    }
    if !mapped.is_empty() {
        text = serde_json::to_string_pretty(&parsed)? + "\n";
    }
    // Where it goes: where it already is inside a bundle directory, else `themes/`.
    let inside = match &b.files {
        scaena_store::Files::Dir(root) => match (root.canonicalize(), theme.canonicalize()) {
            (Ok(root), Ok(theme)) => theme.strip_prefix(&root).ok().map(|p| p.to_string_lossy().replace('\\', "/")),
            _ => None,
        },
        scaena_store::Files::Zip(_) => None,
    };
    let name = theme.file_name().and_then(|n| n.to_str()).context("the theme has no file name")?;
    let rel = inside.clone().unwrap_or_else(|| format!("themes/{name}"));
    let was = match &b.deck.theme {
        Some(serde_json::Value::String(path)) => Some(path.clone()),
        Some(_) => Some("(inline)".to_string()),
        None => None,
    };

    let before = lint(b)?.findings;
    let mut deck = b.deck.clone();
    deck.theme = Some(serde_json::Value::String(rel.clone()));
    let after = lint_in(&deck, &View::of(b).with(rel.clone(), text.clone().into_bytes()))?.findings;

    let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let Delta { added, removed } = delta(&before, &states, &after, &states, &[]);
    let (added, removed) = (added.into_iter().cloned().collect(), removed.into_iter().cloned().collect());
    if !dry_run {
        let mut files = BTreeMap::new();
        if inside.is_none() || !mapped.is_empty() {
            files.insert(rel.clone(), text.into_bytes());
        }
        write_deck(b, &deck, files)?;
    }
    Ok(Themed { theme: rel, was, applied: !dry_run, mapped, added, removed, errors: errors(&after) })
}
