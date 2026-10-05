//! Re-theme a deck (PLAN 1.6, SPEC §3.6): point it at another theme, copied into the bundle,
//! and say what that changes in what `validate` and `lint` find. A theme that would leave
//! the deck invalid is refused, unless forced (PLAN 1.35).

use crate::lint::{View, Why, Write, errors, lint, lint_in, write_deck};
use crate::{Bundle, Context, OpsError};
use scaena_core::Finding;
use scaena_core::document::FontRef;
use scaena_core::lint::{Delta, delta};
use scaena_core::validate::{BundleFiles, validate_bundle};
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
    /// Whether the deck now names the theme: not under a dry run, nor when it was refused.
    pub applied: bool,
    /// Families set in the bundle's font of that family, the theme's own file not being in
    /// the bundle: `family `key`: theirs → ours`.
    pub mapped: Vec<String>,
    /// The theme's families the deck's `fonts` now lists, as rendering needs them to be:
    /// `family `key`: file`. Each is a file the bundle holds that the deck did not list.
    pub listed: Vec<String>,
    /// What `validate` and `lint` find with the new theme that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not with it.
    pub removed: Vec<Finding>,
    /// The findings that are errors, with the new theme.
    pub errors: usize,
    /// Whether the theme was refused: it would have added a validation error (in `added`),
    /// and the swap was not forced. The deck keeps its theme.
    pub refused: bool,
}

/// Point the bundle's deck at the theme file `theme`, copying it to `themes/` unless it is
/// in the bundle already. A family whose file the bundle does not hold is set in the bundle
/// font of that family, if it has one: a saved bundle names fonts by their content. The
/// deck's `fonts` lists each of the theme's families the bundle holds, as rendering needs
/// it to (E102). The deck is not otherwise touched, and is written canonically.
///
/// A theme that lacks a name the deck uses would leave it invalid, and an invalid deck is
/// not laid out, so lint could not say what else the theme breaks. Such a theme is refused,
/// as `patch` refuses an invalid deck, unless `force`: the deck keeps its theme, and the new
/// one is copied in all the same, for one `patch` with the `retheme` op and the fixes.
pub fn theme_apply(b: &Bundle, theme: &Path, dry_run: bool, force: bool) -> Result<Themed, OpsError> {
    let text = std::fs::read_to_string(theme).with_context(|| format!("reading {}", theme.display()))?;
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
    let (themed, write) = theming(b, &rel, &text, &BTreeMap::new(), inside.is_none(), force)?;
    if !dry_run {
        if !themed.refused {
            write_deck(b, &write.deck, write.files, &write.why)?;
        } else if !write.files.is_empty() {
            // The theme, copied in; the deck keeps the one it names.
            b.write(&write.files).with_context(|| format!("writing {}", b.root.display()))?;
        }
    }
    Ok(Themed { applied: !dry_run && !themed.refused, ..themed })
}

/// What a re-theme would do and what it writes, with nothing written: `theme_apply`'s twin,
/// for a bundle held in memory (the web editor, PLAN 2.39). The theme is `text`, at `rel` in
/// the bundle; with `copy`, it is written there. `fonts` are font files the caller can add,
/// by the paths the theme gives them: a family whose file the bundle lacks, and of whose
/// family it holds no font, is set in the one offered. What to write comes back beside what
/// it does: the deck naming the theme, or, refused, the deck as it is, with the theme to
/// copy in all the same. `applied` says whether the write names the theme.
pub fn theming(
    b: &Bundle,
    rel: &str,
    text: &str,
    fonts: &BTreeMap<String, Vec<u8>>,
    copy: bool,
    force: bool,
) -> Result<(Themed, Write), OpsError> {
    let mut parsed: serde_json::Value = serde_json::from_str(text).with_context(|| format!("{rel} is not JSON"))?;
    let mut mapped = Vec::new();
    let mut added = BTreeMap::new();
    for (key, family) in parsed.pointer_mut("/type/families").and_then(|f| f.as_object_mut()).into_iter().flatten() {
        let (Some(file), Some(name)) = (family["file"].as_str(), family["family"].as_str()) else { continue };
        if b.files.exists(file) {
            continue;
        }
        if let Some(font) = b.deck.fonts.iter().find(|f| f.family == name && b.files.exists(&f.file)) {
            mapped.push(format!("family `{key}`: {file} → {}", font.file));
            family["file"] = serde_json::Value::String(font.file.clone());
        } else if let Some(bytes) = fonts.get(file) {
            added.insert(file.to_string(), bytes.clone());
        }
    }
    let text = match mapped.is_empty() {
        true => text.to_string(),
        false => serde_json::to_string_pretty(&parsed)? + "\n",
    };
    let was = match &b.deck.theme {
        Some(serde_json::Value::String(path)) => Some(path.clone()),
        Some(_) => Some("(inline)".to_string()),
        None => None,
    };
    let held = |file: &str| b.files.exists(file) || added.contains_key(file);

    let before = lint(b)?.findings;
    let mut deck = b.deck.clone();
    deck.theme = Some(serde_json::Value::String(rel.to_string()));
    let mut listed = Vec::new();
    for (key, family) in parsed.pointer("/type/families").and_then(|f| f.as_object()).into_iter().flatten() {
        let (Some(file), Some(name)) = (family["file"].as_str(), family["family"].as_str()) else { continue };
        if held(file) && !deck.fonts.iter().any(|f| f.file == file) {
            listed.push(format!("family `{key}`: {file}"));
            let axes = serde_json::from_value(family["axes"].clone()).ok();
            deck.fonts.push(FontRef { family: name.into(), file: file.into(), weight: None, style: None, axes });
        }
    }
    let mut view = View::of(b).with(rel, text.clone().into_bytes());
    for (path, bytes) in &added {
        view = view.with(path.clone(), bytes.clone());
    }
    let after = lint_in(&deck, &view)?.findings;

    let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let invalid = validate_bundle(&b.deck.to_json()?, &View::of(b))?;
    let invalid_after = validate_bundle(&deck.to_json()?, &view)?;
    let refused = !force && !delta(&invalid, &states, &invalid_after, &states, &[]).added.is_empty();
    let Delta { added: worse, removed: better } = delta(&before, &states, &after, &states, &[]);
    let (worse, better) = (worse.into_iter().cloned().collect(), better.into_iter().cloned().collect());
    let mut files = added;
    if copy || !mapped.is_empty() {
        files.insert(rel.to_string(), text.into_bytes());
    }
    let themed = Themed {
        theme: rel.to_string(),
        was,
        applied: !refused,
        mapped,
        listed,
        added: worse,
        removed: better,
        errors: errors(&after),
        refused,
    };
    let deck = if refused { b.deck.clone() } else { deck };
    Ok((themed, Write { deck, files, why: Why::new(format!("theme --apply {rel}")) }))
}
