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
use std::path::{Path, PathBuf};

mod aliases;
/// The names the themes that ship answer to besides their own (PLAN T.0d): `scripts/build_themes.py`
/// writes them from `docs/themes/registry.json`.
pub use aliases::ALIASES;

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
    /// The contrast errors the theme brought that a color of its own fixes, each fixed, one for
    /// each text recolored (PLAN 2.82): a text that read in the deck's theme reads in this one,
    /// set in its color that reads where the text stands. What lint finds after is in `added`.
    pub recolored: Vec<Finding>,
}

/// Point the bundle's deck at the theme file `theme`, copying it to `themes/` unless it is
/// in the bundle already; where no such file is, at the theme that ships by that name, with its
/// fonts, in a build that carries them, as `create` takes one (PLAN T.0d). A family whose file the
/// bundle does not hold is set in the bundle font of that family, if it has one: a saved bundle
/// names fonts by their content. One it has
/// none of comes from the theme's own bundle, beside the theme file, and is copied in with it
/// (`beside`), so a theme in a typeface the deck has never used brings it. The deck's `fonts`
/// lists each of the theme's families the bundle holds, as rendering needs it to (E102). A text the theme leaves too faint to read is set in its color that reads where
/// the text stands (below); the deck is not otherwise touched, and is written canonically.
///
/// A theme that lacks a name the deck uses would leave it invalid, and an invalid deck is
/// not laid out, so lint could not say what else the theme breaks. Such a theme is refused,
/// as `patch` refuses an invalid deck, unless `force`: the deck keeps its theme, and the new
/// one is copied in all the same, for one `patch` with the `retheme` op and the fixes.
///
/// A text the theme leaves too faint to read where it stands, which a contrast error finds (E110,
/// E111), is set in the theme's color that reads there, as the error's fix offers (PLAN 2.82,
/// `recolored`), so a deck reads in a theme it takes as it read in its own.
pub fn theme_apply(
    b: &Bundle,
    theme: &Path,
    fonts_within: Option<&Path>,
    dry_run: bool,
    force: bool,
) -> Result<Themed, OpsError> {
    #[cfg(feature = "shipped")]
    if !theme.exists()
        && let Some(t) = theme.to_str().and_then(crate::shipped::theme)
    {
        let fonts = crate::shipped::fonts().map(|(path, bytes)| (path.to_string(), bytes.to_vec())).collect();
        return applying(b, &format!("themes/{}", t.file), t.text, &fonts, true, dry_run, force);
    }
    let text = std::fs::read_to_string(theme)
        .with_context(|| format!("reading {}{}", theme.display(), crate::create::or_shipped()))?;
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
    let fonts = if inside.is_some() { BTreeMap::new() } else { beside(theme, fonts_within, &text) };
    applying(b, &rel, &text, &fonts, inside.is_none(), dry_run, force)
}

/// The deck put in the theme `text`, at `rel` in the bundle, copied there with `copy`, and
/// written unless `dry_run`.
fn applying(
    b: &Bundle,
    rel: &str,
    text: &str,
    fonts: &BTreeMap<String, Vec<u8>>,
    copy: bool,
    dry_run: bool,
    force: bool,
) -> Result<Themed, OpsError> {
    let (themed, write) = theming(b, rel, text, fonts, copy, force, true)?;
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

/// The font files `text`, the theme file at `theme`, names, as its own bundle holds them: each
/// path from the theme's folder, or from the folder above it, a bundle's `themes/` being in its
/// root, and no higher than `within`, taking it in, where one is given (see
/// [`crate::create::Create::fonts_within`]). A face found in neither is left to the bundle being
/// themed.
fn beside(theme: &Path, within: Option<&Path>, text: &str) -> BTreeMap<String, Vec<u8>> {
    let mut fonts = BTreeMap::new();
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) else { return fonts };
    let folder = theme.parent().unwrap_or(Path::new("."));
    let families = parsed.pointer("/type/families").and_then(|f| f.as_object());
    for family in families.into_iter().flat_map(|f| f.values()) {
        for face in [Some(family), family.get("italic")].into_iter().flatten() {
            let Some(file) = face.get("file").and_then(|f| f.as_str()) else { continue };
            if crate::create::fonts_named(file).is_err() {
                continue;
            }
            // A link out of the folder is passed over as a face not found.
            let dirs = [folder.to_path_buf(), folder.join("..")];
            let held = |dir: &&PathBuf| within.is_none_or(|w| dir.canonicalize().is_ok_and(|d| d.starts_with(w)));
            let found = crate::create::font_in(dirs.iter().filter(held).map(|d| d.as_path()), file)
                .ok()
                .flatten()
                .and_then(|p| std::fs::read(p).ok());
            if let Some(bytes) = found {
                fonts.insert(file.to_string(), bytes);
            }
        }
    }
    fonts
}

/// What a re-theme would do and what it writes, with nothing written: `theme_apply`'s twin,
/// for a bundle held in memory (the web editor, PLAN 2.39). The theme is `text`, at `rel` in
/// the bundle; with `copy`, it is written there. `fonts` are font files the caller can add,
/// by the paths the theme gives them: a face (a family's own, or its italic) whose file the
/// bundle lacks, and of whose family and style it holds no font, is set in the one offered.
/// What to write comes back beside what it does: the deck naming the theme, or, refused, the
/// deck as it is, with the theme to copy in all the same. `applied` says whether the write
/// names the theme. The family the deck's theme took for a script's font, the new theme takes
/// too, and is written with it (PLAN G.22). With `recolor`, each text the theme leaves too faint
/// to read is set in its color that reads where the text stands (PLAN 2.82), as a re-theme does;
/// a theme edit (`theme_editing`) changes the theme alone.
pub fn theming(
    b: &Bundle,
    rel: &str,
    text: &str,
    fonts: &BTreeMap<String, Vec<u8>>,
    copy: bool,
    force: bool,
    recolor: bool,
) -> Result<(Themed, Write), OpsError> {
    // A deck in a script the theme's own fonts do not draw keeps the script's font (PLAN G.22).
    let carried = crate::scripts::carried(b.theme_json.as_deref(), text)?;
    let scripted = carried != text;
    let text = carried.as_str();
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
    let mut after = lint_in(&deck, &view)?.findings;

    let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let invalid = validate_bundle(&b.deck.to_json()?, &View::of(b))?;
    let invalid_after = validate_bundle(&deck.to_json()?, &view)?;
    let refused = !force && !delta(&invalid, &states, &invalid_after, &states, &[]).added.is_empty();
    let mut recolored = Vec::new();
    if recolor && !refused {
        let lint = |deck: &Deck| Ok(lint_in(deck, &view)?.findings);
        let done = recoloring(deck, &before, &states, after, lint)?;
        (deck, after, recolored) = (done.deck, done.after, done.recolored);
    }
    let Delta { added: worse, removed: better } = delta(&before, &states, &after, &states, &[]);
    let (worse, better) = (worse.into_iter().cloned().collect(), better.into_iter().cloned().collect());
    let mut files = added;
    if copy || scripted || !mapped.is_empty() {
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
        recolored,
    };
    let deck = if refused { b.deck.clone() } else { deck };
    Ok((themed, Write { deck, files, why: Why::new(format!("theme --apply {rel}")) }))
}

/// A deck [`recoloring`] set in colors that read.
pub(crate) struct Recolored {
    /// The deck so recolored.
    pub deck: Deck,
    /// What lint finds in it.
    pub after: Vec<Finding>,
    /// The errors fixed, one for each text.
    pub recolored: Vec<Finding>,
    /// The fixes' ops, in the order they applied.
    pub ops: Vec<Value>,
}

/// `deck` with each contrast error a change brought (in `after` and not in `before`) fixed as its
/// fix offers: the text set in the theme's color that reads where it stands (PLAN 2.82), as a
/// re-theme and a slide's background (PLAN 2.148) set it. `lint` is what lint finds in a deck. Two
/// fixes that make one container in one state could each write it whole, so lint lays the deck out
/// again and fixes what is left, three rounds at most.
pub(crate) fn recoloring(
    mut deck: Deck,
    before: &[Finding],
    states: &[&str],
    mut after: Vec<Finding>,
    lint: impl Fn(&Deck) -> Result<Vec<Finding>, OpsError>,
) -> Result<Recolored, OpsError> {
    let mut recolored: Vec<Finding> = Vec::new();
    let mut ops: Vec<Value> = Vec::new();
    for _ in 0..3 {
        let faint: Vec<Finding> = (delta(before, states, &after, states, &[]).added.into_iter())
            .filter(|f| matches!(f.code.as_str(), "E110" | "E111") && f.fix.is_some())
            .cloned()
            .collect();
        let mut doc = deck.to_value()?;
        let mut applied: Vec<&[Value]> = Vec::new();
        for f in &faint {
            // A text's errors in each format share one fix, which applies once.
            let fix = f.fix.as_deref().unwrap_or_default();
            if applied.contains(&fix) {
                continue;
            }
            let mut next = doc.clone();
            if scaena_core::patch::apply(&mut next, fix).is_ok() {
                doc = next;
                applied.push(fix);
                ops.extend(fix.iter().cloned());
                if !recolored.iter().any(|r| r.node == f.node) {
                    recolored.push(f.clone());
                }
            }
        }
        if applied.is_empty() {
            break;
        }
        deck = Deck::from_json(&doc.to_string()).context("the deck, recolored")?;
        after = lint(&deck)?;
    }
    Ok(Recolored { deck, after, recolored, ops })
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
            let (themed, mut w) = theming(b, rel, &text, &BTreeMap::new(), true, false, false)?;
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

/// What bringing a theme's layout library into an older copy of it did (PLAN 3.31).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Library {
    /// The theme's name: that of the theme that ships, which the copy shares.
    pub theme: String,
    /// The layouts added, by their keys, in the library's order.
    pub added: Vec<String>,
    /// The roles added, by their names: those the layouts added set words in.
    pub roles: Vec<String>,
    /// The layouts the copy had that were given a name, a section, a prompt, or an example.
    pub named: Vec<String>,
    /// Whether the slide's number and footer, which the copy placed nowhere, were placed as the
    /// theme ships them (ADR-0028).
    pub furniture: bool,
    /// The builds added (PLAN 2.112): the motion presets the theme ships that the copy lacked, by
    /// their names, in the theme's order.
    pub builds: Vec<String>,
    /// The springs added, by their names: those the builds added spring on.
    pub springs: Vec<String>,
    /// The shadows added (PLAN 2.115): those the theme ships that the copy lacked, by their names,
    /// in the theme's order.
    pub shadows: Vec<String>,
    /// Whether the look the theme gives code (PLAN 2.132), which the copy lacked, was brought: only
    /// into a copy whose deck shows no code text, so no code draws otherwise than it did.
    pub code: bool,
    /// The backgrounds added (PLAN 2.147): the shader presets the theme ships that the copy
    /// lacked, by their names, in the theme's order, each with its palette where the copy lacked
    /// that.
    pub backgrounds: Vec<String>,
}

/// Whether a theme named `copy` is the theme that ships as `name`: the same name, or one it
/// shipped under before (PLAN T.0d), so a bundle's copy of Gallery is Salon's.
pub fn same(copy: &str, name: &str) -> bool {
    copy == name || ALIASES.iter().any(|(alias, is)| alias.eq_ignore_ascii_case(copy) && is.eq_ignore_ascii_case(name))
}

/// `theme`, a bundle's copy of the theme that ships as `shipped`, with the layout library that
/// theme ships now brought in (PLAN 3.31, SPEC §3.6): what an editor opens a deck in where its
/// theme was copied into its bundle before the library grew, so Add Slide offers every layout, by
/// section, each drawn as it is used. Added: each layout the copy lacks, as the theme ships it,
/// and each role those set words in that the copy lacks; and to each layout it has, the name,
/// section, and description the library gives it, and to each of its slots the prompt and the
/// example, where the copy has none. No slot of a layout it has moves, and no role, token, or
/// other part of it changes, so every slide draws as it did. A layout that sets words in a role
/// the copy lacks and cannot take, that role's family or color not being the copy's, is left out,
/// and an example drawn by a shader preset the copy lacks is left off. The layouts go in the
/// library's order, then the copy's own, so the gallery's sections come as in a deck started in
/// the theme, and the theme is written in the library's format. A copy that places no slide
/// number or footer takes the theme's placement of them (ADR-0028) where it has their roles and
/// colors, left off the layouts it has of those the theme leaves them off; a deck shows them only
/// where it asks. The builds the theme ships that the copy lacks are added too (PLAN 2.112), each
/// motion preset whose duration, easing, and color the copy names, with the spring it springs on
/// where the copy lacks that, so the cue offers every build a deck started in the theme does. The
/// look the theme gives code, which the copy lacks, comes in only where the deck shows no code
/// text, `code_shown` false (PLAN 2.132): a code text it shows keeps the colors it is drawn in. The
/// backgrounds the theme ships that the copy lacks come in with their palettes (PLAN 2.147), so a
/// background picker offers each. None where `theme` is not a copy of `shipped`, another name or
/// another grid, or has every layout, name, placement, build, look, and background it would bring.
pub fn library(theme: &Value, shipped: &Value, code_shown: bool) -> Option<(Value, Library)> {
    let name = shipped.get("name")?.as_str()?;
    // A copy named as the theme shipped before is its copy too: Gallery's is Salon's (PLAN T.0d).
    if !same(theme.get("name")?.as_str()?, name) {
        return None;
    }
    let tracks = |t: &Value| (t.pointer("/grid/columns").cloned(), t.pointer("/grid/rows").cloned());
    if tracks(theme) != tracks(shipped) {
        return None;
    }
    let ours = theme.get("layouts").and_then(Value::as_object).cloned().unwrap_or_default();
    let theirs = shipped.get("layouts")?.as_object()?;
    let roles = theme.pointer("/type/roles").and_then(Value::as_object).cloned().unwrap_or_default();
    let families = theme.pointer("/type/families").and_then(Value::as_object);
    let colors = theme.pointer("/tokens/color").and_then(Value::as_object);
    let presets = theme.pointer("/shaders/presets").and_then(Value::as_object);
    // A role the copy lacks that it can take from the library: one in a family and a color the
    // copy has.
    let takes = |role: &str| -> Option<Value> {
        let r = shipped.get("type")?.get("roles")?.get(role)?;
        let family =
            r.get("family").and_then(Value::as_str).is_none_or(|f| families.is_some_and(|fs| fs.contains_key(f)));
        let color = (r.get("color").and_then(Value::as_str))
            .is_none_or(|c| c.starts_with('#') || colors.is_some_and(|cs| cs.contains_key(c)));
        (family && color).then(|| r.clone())
    };
    let mut brought = Library { theme: name.to_string(), ..Library::default() };
    let mut taken = serde_json::Map::new();
    let mut layouts = serde_json::Map::new();
    for (key, layout) in theirs {
        if let Some(own) = ours.get(key) {
            let (own, named) = given(own, layout, presets);
            if named {
                brought.named.push(key.clone());
            }
            layouts.insert(key.clone(), own);
            continue;
        }
        // Each role it sets words in that the copy lacks, taken; or, one it cannot take, not it.
        let missing = set_in(layout).into_iter().filter(|r| !roles.contains_key(r) && !taken.contains_key(r));
        let Some(wanted) = missing.map(|r| takes(&r).map(|v| (r, v))).collect::<Option<Vec<_>>>() else { continue };
        for (role, r) in wanted {
            brought.roles.push(role.clone());
            taken.insert(role, r);
        }
        brought.added.push(key.clone());
        layouts.insert(key.clone(), drawable(layout, presets));
    }
    for (key, own) in ours {
        layouts.entry(key).or_insert(own);
    }
    // The slide's number and footer, where the copy places neither: in roles and colors it has.
    let furniture = match theme.get("furniture") {
        Some(_) => None,
        None => shipped.get("furniture").and_then(|f| placed(f, theme, &taken, &layouts)),
    };
    brought.furniture = furniture.is_some();
    let builds = builds(theme, shipped);
    if let Some((_, _, added, springs)) = &builds {
        brought.builds = added.clone();
        brought.springs = springs.clone();
    }
    let shadows = shadows(theme, shipped);
    if let Some((_, added)) = &shadows {
        brought.shadows = added.clone();
    }
    let backgrounds = backgrounds(theme, shipped);
    if let Some((_, _, added)) = &backgrounds {
        brought.backgrounds = added.clone();
    }
    // Code's look (PLAN 2.132), where the copy has none and nothing in the deck draws in the one it
    // has: the theme's own colors would recolor code a slide shows.
    let code = shipped.get("code").filter(|_| theme.get("code").is_none() && !code_shown);
    brought.code = code.is_some();
    if brought.added.is_empty()
        && brought.named.is_empty()
        && !brought.furniture
        && builds.is_none()
        && shadows.is_none()
        && backgrounds.is_none()
        && code.is_none()
    {
        return None;
    }
    let mut theme = theme.clone();
    theme["layouts"] = Value::Object(layouts);
    if let Some((presets, springs, _, _)) = builds {
        theme["motion"]["presets"] = Value::Object(presets);
        theme["motion"]["springs"] = Value::Object(springs);
    }
    if !taken.is_empty() {
        if theme.pointer("/type/roles").is_none() {
            theme["type"]["roles"] = Value::Object(serde_json::Map::new());
        }
        let roles = theme.pointer_mut("/type/roles")?.as_object_mut()?;
        roles.extend(taken);
    }
    if let Some(furniture) = furniture {
        theme["furniture"] = furniture;
    }
    if let Some((all, _)) = shadows {
        theme["tokens"]["shadow"] = Value::Object(all);
    }
    if let Some((presets, palettes, _)) = backgrounds {
        if !theme.get("shaders").is_some_and(Value::is_object) {
            theme["shaders"] = Value::Object(serde_json::Map::new());
        }
        theme["shaders"]["presets"] = Value::Object(presets);
        theme["shaders"]["palettes"] = Value::Object(palettes);
    }
    if let Some(code) = code {
        theme["code"] = code.clone();
    }
    if let Some(version) = shipped.get("scaena-theme") {
        theme["scaena-theme"] = version.clone();
    }
    Some((theme, brought))
}

/// The shadows `copy` takes from `shipped` (PLAN 2.115): every one the theme ships, in its order,
/// the copy's own where it has one of that name, then the copy's others; and the names of those
/// added. None where the copy lacks none.
fn shadows(copy: &Value, shipped: &Value) -> Option<(serde_json::Map<String, Value>, Vec<String>)> {
    let theirs = shipped.pointer("/tokens/shadow")?.as_object()?;
    let ours = copy.pointer("/tokens/shadow").and_then(Value::as_object).cloned().unwrap_or_default();
    let added: Vec<String> = theirs.keys().filter(|k| !ours.contains_key(*k)).cloned().collect();
    if added.is_empty() {
        return None;
    }
    let mut all = serde_json::Map::new();
    for (name, shadow) in theirs {
        all.insert(name.clone(), ours.get(name).unwrap_or(shadow).clone());
    }
    for (name, shadow) in ours {
        all.entry(name).or_insert(shadow);
    }
    Some((all, added))
}

/// A copy's shader presets and palettes with the theme's backgrounds brought in, and the names of
/// the presets added.
type Backgrounds = (serde_json::Map<String, Value>, serde_json::Map<String, Value>, Vec<String>);

/// The shader presets `copy` takes from `shipped` (PLAN 2.147), and their palettes: every preset
/// the theme ships, in its order, the copy's own where it has one of that name, then the copy's
/// others; each palette an added preset names that the copy lacks, after the copy's own; and the
/// names of the presets added. None where the copy lacks none.
fn backgrounds(copy: &Value, shipped: &Value) -> Option<Backgrounds> {
    let theirs = shipped.pointer("/shaders/presets")?.as_object()?;
    let ours = copy.pointer("/shaders/presets").and_then(Value::as_object).cloned().unwrap_or_default();
    let added: Vec<String> = theirs.keys().filter(|k| !ours.contains_key(*k)).cloned().collect();
    if added.is_empty() {
        return None;
    }
    let mut presets = serde_json::Map::new();
    for (name, preset) in theirs {
        presets.insert(name.clone(), ours.get(name).unwrap_or(preset).clone());
    }
    for (name, preset) in ours {
        presets.entry(name).or_insert(preset);
    }
    let mut palettes = copy.pointer("/shaders/palettes").and_then(Value::as_object).cloned().unwrap_or_default();
    for name in &added {
        let palette = theirs[name].get("palette").and_then(Value::as_str);
        if let Some(colors) = palette.and_then(|p| shipped.pointer("/shaders/palettes")?.get(p)) {
            palettes.entry(palette.unwrap_or_default()).or_insert_with(|| colors.clone());
        }
    }
    Some((presets, palettes, added))
}

/// `furniture`, the theme's as it ships, for `copy` (taking the roles `taken` too), its layouts
/// being `layouts`: each piece in a role and a color the copy has, the layouts it is left off those
/// the copy has. None where neither piece can be placed.
fn placed(
    furniture: &Value,
    copy: &Value,
    taken: &serde_json::Map<String, Value>,
    layouts: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let has = |at: &str, name: &str| copy.pointer(at).and_then(Value::as_object).is_some_and(|m| m.contains_key(name));
    let mut out = furniture.as_object()?.clone();
    for key in ["number", "footer"] {
        let fits = out.get(key).is_some_and(|item| {
            let role = (item.get("role").and_then(Value::as_str))
                .is_some_and(|r| has("/type/roles", r) || taken.contains_key(r));
            let color = (item.get("color").and_then(Value::as_str))
                .is_none_or(|c| c.starts_with('#') || has("/tokens/color", c) || has("/tokens/roles", c));
            role && color
        });
        if !fits {
            out.remove(key);
        }
    }
    if !out.contains_key("number") && !out.contains_key("footer") {
        return None;
    }
    if let Some(Value::Array(except)) = out.get_mut("except") {
        except.retain(|l| l.as_str().is_some_and(|l| layouts.contains_key(l)));
    }
    Some(Value::Object(out))
}

/// The motion presets of `copy` with the builds `shipped` has that it lacks (PLAN 2.112): each
/// whose duration, easing, and color `copy` names, the theme's order first, then the copy's own;
/// and its springs, with each that a build added springs on and the copy lacks. Then the names of
/// the builds and the springs added. None where it lacks none it can take.
#[allow(clippy::type_complexity)]
fn builds(
    copy: &Value,
    shipped: &Value,
) -> Option<(serde_json::Map<String, Value>, serde_json::Map<String, Value>, Vec<String>, Vec<String>)> {
    let named = |at: &str| copy.pointer(at).and_then(Value::as_object).cloned().unwrap_or_default();
    let (ours, mut springs) = (named("/motion/presets"), named("/motion/springs"));
    let (durations, easings) = (named("/motion/durations"), named("/motion/easings"));
    let theirs = shipped.pointer("/motion/presets")?.as_object()?;
    let their_springs = shipped.pointer("/motion/springs").and_then(Value::as_object);
    let has = |at: &str, name: &str| copy.pointer(at).and_then(Value::as_object).is_some_and(|m| m.contains_key(name));
    let (mut presets, mut added, mut sprung) = (serde_json::Map::new(), Vec::new(), Vec::new());
    for (name, preset) in theirs {
        if let Some(own) = ours.get(name) {
            presets.insert(name.clone(), own.clone());
            continue;
        }
        // What it names: a duration, an easing, and a color the copy has, and a spring it has or
        // can take.
        let says = |key: &str, in_: &serde_json::Map<String, Value>| {
            preset.get(key).and_then(Value::as_str).is_none_or(|n| in_.contains_key(n))
        };
        let colored = ["from", "to"].iter().all(|look| {
            (preset.get(*look).and_then(|l| l.get("color")).and_then(Value::as_str))
                .is_none_or(|c| c.starts_with('#') || has("/tokens/color", c) || has("/tokens/roles", c))
        });
        let spring = preset.get("spring").and_then(Value::as_str);
        let springs_on =
            spring.is_none_or(|n| springs.contains_key(n) || their_springs.is_some_and(|s| s.contains_key(n)));
        if !(says("duration", &durations) && says("ease", &easings) && colored && springs_on) {
            continue;
        }
        if let Some(n) = spring.filter(|n| !springs.contains_key(*n))
            && let Some(s) = their_springs.and_then(|s| s.get(n))
        {
            springs.insert(n.to_string(), s.clone());
            sprung.push(n.to_string());
        }
        presets.insert(name.clone(), preset.clone());
        added.push(name.clone());
    }
    if added.is_empty() {
        return None;
    }
    for (name, own) in ours {
        presets.entry(name).or_insert(own);
    }
    Some((presets, springs, added, sprung))
}

/// The roles a layout sets words in: its slots', and those of its slots in each format.
fn set_in(layout: &Value) -> Vec<String> {
    let mut slots: Vec<&Value> =
        layout.get("slots").and_then(Value::as_object).into_iter().flat_map(|s| s.values()).collect();
    for format in layout.get("formats").and_then(Value::as_object).into_iter().flat_map(|f| f.values()) {
        slots.extend(format.get("slots").and_then(Value::as_object).into_iter().flat_map(|s| s.values()));
    }
    let mut roles: Vec<String> = Vec::new();
    for role in slots.iter().filter_map(|s| s.get("role").and_then(Value::as_str)) {
        if !roles.iter().any(|r| r == role) {
            roles.push(role.to_string());
        }
    }
    roles
}

/// Whether the copy can draw `example`, a slot's: a picture by a shader preset it has, or a chart
/// or a table.
fn draws(example: &Value, presets: Option<&serde_json::Map<String, Value>>) -> bool {
    example.get("picture").and_then(Value::as_str).is_none_or(|p| presets.is_some_and(|ps| ps.contains_key(p)))
}

/// `layout`, the library's, with each example the copy cannot draw left off.
fn drawable(layout: &Value, presets: Option<&serde_json::Map<String, Value>>) -> Value {
    let mut out = layout.clone();
    for slot in out.get_mut("slots").and_then(Value::as_object_mut).into_iter().flat_map(|s| s.values_mut()) {
        if let Some(s) = slot.as_object_mut()
            && s.get("example").is_some_and(|e| !draws(e, presets))
        {
            s.remove("example");
        }
    }
    out
}

/// `own`, a layout the copy has, given what the library's `theirs` says of it where it says
/// nothing: its name, section, and description, and each slot's prompt and example, where that
/// slot takes words as the library's does or a picture as it does. Whether it was given any.
fn given(own: &Value, theirs: &Value, presets: Option<&serde_json::Map<String, Value>>) -> (Value, bool) {
    let mut out = own.clone();
    let mut named = false;
    let Some(layout) = out.as_object_mut() else { return (out, false) };
    for key in ["name", "group", "description"] {
        if !layout.contains_key(key)
            && let Some(v) = theirs.get(key)
        {
            layout.insert(key.to_string(), v.clone());
            named = true;
        }
    }
    let their_slots = theirs.get("slots").and_then(Value::as_object);
    for (key, slot) in layout.get_mut("slots").and_then(Value::as_object_mut).into_iter().flatten() {
        let (Some(s), Some(t)) = (slot.as_object_mut(), their_slots.and_then(|ts| ts.get(key))) else { continue };
        if s.contains_key("role") != t.get("role").is_some() {
            continue;
        }
        for field in ["prompt", "example"] {
            if !s.contains_key(field)
                && let Some(v) = t.get(field)
                && (field != "example" || draws(v, presets))
            {
                s.insert(field.to_string(), v.clone());
                named = true;
            }
        }
    }
    (out, named)
}
