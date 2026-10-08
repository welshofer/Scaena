//! Re-theme a deck (PLAN 1.6, SPEC §3.6): point it at another theme, copied into the bundle,
//! and say what that changes in what `validate` and `lint` find. A theme that would leave
//! the deck invalid is refused, unless forced (PLAN 1.35).
//!
//! Or edit the theme the deck names (PLAN 2.61, ADR-0016): RFC 6902 operations on its JSON,
//! checked as a re-theme is, and written in canonical form, as `save` writes it.

use crate::lint::{View, Why, Write, errors, lint, lint_in, write, write_deck};
use crate::{Bundle, Context, OpsError};
use scaena_core::document::FontRef;
use scaena_core::lint::{Delta, delta};
use scaena_core::validate::{BundleFiles, validate_bundle};
use scaena_core::{Deck, Finding};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
/// by the paths the theme gives them: a face (a family's own, or its italic) whose file the
/// bundle lacks, and of whose family and style it holds no font, is set in the one offered.
/// What to write comes back beside what it does: the deck naming the theme, or, refused, the
/// deck as it is, with the theme to copy in all the same. `applied` says whether the write
/// names the theme.
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
        let Some(name) = family["family"].as_str().map(String::from) else { continue };
        // The family's own face, and its italic (PLAN 2.40).
        for italic in [false, true] {
            let face = if italic { family.get_mut("italic") } else { Some(&mut *family) };
            let Some(face) = face else { continue };
            let Some(file) = face.get("file").and_then(|f| f.as_str()).map(String::from) else { continue };
            if b.files.exists(&file) {
                continue;
            }
            let held = (b.deck.fonts.iter()).find(|f| {
                f.family == name && (f.style.as_deref() == Some("italic")) == italic && b.files.exists(&f.file)
            });
            if let Some(font) = held {
                let what = if italic { " italic" } else { "" };
                mapped.push(format!("family `{key}`{what}: {file} → {}", font.file));
                face["file"] = serde_json::Value::String(font.file.clone());
            } else if let Some(bytes) = fonts.get(&file) {
                added.insert(file, bytes.clone());
            }
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
    let listed = list_fonts(&mut deck, &parsed, held);
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

/// Each of `theme`'s families, and its italic, whose file `held` says the bundle holds and
/// `deck`'s `fonts` does not list, listed there, as rendering needs it to be (E102): what was
/// listed, `family `key`: file`.
fn list_fonts(deck: &mut Deck, theme: &Value, held: impl Fn(&str) -> bool) -> Vec<String> {
    let mut listed = Vec::new();
    for (key, family) in theme.pointer("/type/families").and_then(|f| f.as_object()).into_iter().flatten() {
        let Some(name) = family["family"].as_str() else { continue };
        for (face, style) in [(family, None), (&family["italic"], Some("italic"))] {
            let Some(file) = face["file"].as_str() else { continue };
            if held(file) && !deck.fonts.iter().any(|f| f.file == file) {
                let what = if style.is_some() { " italic" } else { "" };
                listed.push(format!("family `{key}`{what}: {file}"));
                let axes = serde_json::from_value(face["axes"].clone()).ok();
                let style = style.map(String::from);
                deck.fonts.push(FontRef { family: name.into(), file: file.into(), weight: None, style, axes });
            }
        }
    }
    listed
}

/// What a theme edit asks (ADR-0016): what `scaena theme --edit` and `--from-photo` read and
/// `theme_edit` takes.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ThemeEdit {
    /// RFC 6902 operations on the theme the deck names, applied in order, all or none: each
    /// path a JSON Pointer into the theme's JSON (`/tokens/color/accent`,
    /// `/type/roles/body/size`, `/grid/gutter`), for a theme file and an inline theme alike.
    #[serde(default)]
    pub ops: Vec<Value>,
    /// In place of `ops`: an image the bundle holds (`assets/ridge.png`), whose colors the
    /// theme's take (PLAN 2.94). Its best hue becomes the accent's, and the best far enough from
    /// it the next chromatic color's. The hue the whole photo leans to tints the neutrals. Each
    /// color keeps the theme's tone, its lightness and chroma, and each color text is set in is
    /// moved until it reads on the surfaces. The result's `photo` says what it read and set.
    #[serde(default)]
    pub photo: Option<String>,
}

/// What a theme edit did, or would do.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ThemeEdited {
    /// The theme edited: its path in the bundle, or `(inline)` for one written in the deck.
    pub theme: String,
    /// Where in the theme the edit writes: each operation's path, once, in order.
    pub paths: Vec<String>,
    /// Whether the edit stands: not under a dry run, nor when it was refused.
    pub applied: bool,
    /// The theme's families the deck's `fonts` now lists, as rendering needs them to be:
    /// `family `key`: file`. Each is a file the bundle holds that the deck did not list.
    pub listed: Vec<String>,
    /// What `validate` and `lint` find in the theme as edited that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not after.
    pub removed: Vec<Finding>,
    /// The findings that are errors, after.
    pub errors: usize,
    /// Whether the edit was refused: it would have added a validation error (in `added`). The
    /// theme stays as it was.
    pub refused: bool,
    /// With `photo`: what its colors are, and each of the theme's colors and data palettes, what
    /// it was and what it takes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<Photo>,
}

/// What a theme edit from a photo read and set (PLAN 2.94, `scaena_core::palette`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Photo {
    /// The image, as the bundle names it.
    pub image: String,
    /// What its colors are: its hues, best first, and the hue it leans to.
    pub read: scaena_core::palette::Read,
    /// Each of the theme's colors and data palettes: what it was, what it takes, and, for a
    /// color text is set in, the least contrast it has on the surfaces and the least it needs.
    pub set: Vec<scaena_core::palette::Set>,
}

/// The colors the image `image` gives the theme the deck names (PLAN 2.94).
fn photo(b: &Bundle, image: &str) -> Result<Photo, OpsError> {
    let bytes = b.read(image).with_context(|| format!("reading the photo, {image}"))?;
    let picture = scaena_paint::Picture::decode(&bytes)
        .map_err(|e| OpsError::new(format!("{image} is not a photo to read, a PNG or a JPEG: {e}")))?;
    let read = scaena_core::palette::read(picture.rgba.data(), picture.width, picture.height);
    let theme = match &b.deck.theme {
        Some(Value::String(rel)) => {
            let bytes = b.read(rel).with_context(|| format!("reading the theme, {rel}"))?;
            serde_json::from_slice(&bytes).with_context(|| format!("{rel} is not JSON"))?
        }
        Some(inline @ Value::Object(_)) => inline.clone(),
        _ => return Err(OpsError::new("the deck names no theme to edit: `theme --apply` gives it one")),
    };
    let set = scaena_core::palette::theme(&theme, &read).map_err(OpsError::new)?;
    Ok(Photo { image: image.to_string(), read, set })
}

/// Edit the theme the deck names by `ops` (PLAN 2.61, ADR-0016): a theme file, written in
/// canonical form as `save` writes it, or an inline theme, written into the deck; a theme that
/// ships, in the bundle's copy. Refused, as a re-theme is, where the deck would not validate
/// in the theme it leaves: a name the deck uses taken out of it, or a theme its schema refuses.
/// Recorded in the bundle's history, if it keeps one, with the theme's bytes. A dry run writes
/// nothing.
pub fn theme_edit(b: &Bundle, edit: &ThemeEdit, dry_run: bool) -> Result<ThemeEdited, OpsError> {
    let (edited, write_it) = theme_editing(b, edit)?;
    if let Some(w) = write_it.filter(|_| !dry_run) {
        write(b, w)?;
    }
    Ok(ThemeEdited { applied: edited.applied && !dry_run, ..edited })
}

/// [`theme_edit`] with nothing written: what the edit does, and what to write, if it changes
/// the theme and is not refused. A client that keeps its bundle in memory, as the web editor
/// does, writes it there.
pub fn theme_editing(b: &Bundle, edit: &ThemeEdit) -> Result<(ThemeEdited, Option<Write>), OpsError> {
    let (ops, photo) = match (&edit.photo, edit.ops.is_empty()) {
        (Some(_), false) => return Err(OpsError::new("a theme edit is `ops` or a `photo`, not both")),
        (Some(image), true) => {
            let photo = photo(b, image)?;
            let ops = scaena_core::palette::ops(&photo.set);
            if ops.is_empty() {
                return Err(OpsError::new(format!("the theme's colors are {image}'s already: nothing to edit")));
            }
            (ops, Some(photo))
        }
        (None, true) => {
            return Err(OpsError::new(
                "no operations: a theme edit is a list of JSON Patch operations (RFC 6902), or a `photo`",
            ));
        }
        (None, false) => (edit.ops.clone(), None),
    };
    let failed = |e: scaena_core::patch::PatchError| OpsError { message: e.to_string(), plan: None, op: Some(e.index) };
    let mut paths: Vec<String> = Vec::new();
    for path in ops.iter().filter_map(|op| op.get("path").and_then(Value::as_str)) {
        if !paths.iter().any(|p| p == path) {
            paths.push(path.to_string());
        }
    }
    let shown: Vec<&str> = paths.iter().map(|p| p.strip_prefix('/').unwrap_or(p)).collect();
    let why = Why::new(match &photo {
        Some(photo) => format!("theme_edit: colors from {}", photo.image),
        None => format!("theme_edit: {}", shown.join(", ")),
    });
    match &b.deck.theme {
        Some(Value::String(rel)) => {
            let bytes = b.read(rel).with_context(|| format!("reading the theme, {rel}"))?;
            let was: Value = serde_json::from_slice(&bytes).with_context(|| format!("{rel} is not JSON"))?;
            let mut theme = was.clone();
            scaena_core::patch::apply(&mut theme, &ops).map_err(failed)?;
            let text = serde_json::to_string_pretty(&theme)? + "\n";
            let (themed, mut w) = theming(b, rel, &text, &BTreeMap::new(), true, false)?;
            // The deck changes only where it lists a font the theme now names.
            let changes = theme != was || !themed.listed.is_empty();
            let edited = ThemeEdited {
                theme: rel.clone(),
                paths,
                applied: !themed.refused,
                listed: themed.listed,
                added: themed.added,
                removed: themed.removed,
                errors: themed.errors,
                refused: themed.refused,
                photo,
            };
            w.why = why;
            let write_it = (!edited.refused && changes).then_some(w);
            Ok((edited, write_it))
        }
        Some(Value::Object(inline)) => {
            let was = Value::Object(inline.clone());
            let mut theme = was.clone();
            scaena_core::patch::apply(&mut theme, &ops).map_err(failed)?;
            let mut deck = b.deck.clone();
            let listed = list_fonts(&mut deck, &theme, |file| b.files.exists(file));
            deck.theme = Some(theme.clone());
            let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
            let invalid = validate_bundle(&b.deck.to_json()?, &b.files)?;
            let invalid_after = validate_bundle(&deck.to_json()?, &b.files)?;
            let refused = !delta(&invalid, &states, &invalid_after, &states, &[]).added.is_empty();
            let (before, after) = match refused {
                true => (invalid, invalid_after),
                false => (lint(b)?.findings, lint_in(&deck, &View::of(b))?.findings),
            };
            let Delta { added, removed } = delta(&before, &states, &after, &states, &[]);
            let edited = ThemeEdited {
                theme: "(inline)".into(),
                paths,
                applied: !refused,
                listed,
                added: added.into_iter().cloned().collect(),
                removed: removed.into_iter().cloned().collect(),
                errors: errors(&after),
                refused,
                photo,
            };
            let changes = theme != was || !edited.listed.is_empty();
            let write_it = (!refused && changes).then(|| Write::new(deck, why));
            Ok((edited, write_it))
        }
        _ => Err(OpsError::new("the deck names no theme to edit: `theme --apply` gives it one")),
    }
}
