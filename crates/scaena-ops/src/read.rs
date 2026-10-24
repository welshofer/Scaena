//! Read a deck and its spine, and replace the spine (SPEC §2.6, §3.11).

use crate::lint::Write;
use crate::patch::Patched;
use crate::{Bundle, Context, OpsError};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

/// A deck, as `deck.json` holds it or as `.scn`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Read {
    /// The deck, canonical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck: Option<Value>,
    /// The deck as canonical `.scn` (SPEC §4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scn: Option<String>,
    /// With `files`, the bundle's images, fonts, and data, each with what in the deck names it
    /// and the nodes drawn from it, as `scaena files` lists them (PLAN 2.59).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<scaena_core::files::BundleFile>>,
    /// With `theme`, the theme the deck draws in, as the bundle holds it (SPEC §3.6): its
    /// colors, type roles, layouts and their slots, and presets, by the names the deck uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Value>,
}

/// The bundle's deck: as JSON, or with `scn`, as `.scn`; with `files`, the bundle's images,
/// fonts, and data too; with `theme`, the theme it draws in (dogfood 3).
pub fn read(b: &Bundle, scn: bool, files: bool, theme: bool) -> Result<Read, OpsError> {
    let files = if files { Some(crate::files::files(b)?) } else { None };
    let theme = match theme {
        true => {
            let text = b.theme_json.as_deref().context("the deck names no theme")?;
            Some(serde_json::from_str(text).context("the bundle's theme")?)
        }
        false => None,
    };
    Ok(match scn {
        true => Read { deck: None, scn: Some(scaena_core::dsl::decompile(&b.deck)), files, theme },
        false => Read { deck: Some(b.deck.to_value()?), scn: None, files, theme },
    })
}

/// The deck's spine projection (SPEC §10): its sections and beats, each beat with its
/// states and the one that shows it (SPEC §3.11), as `export --format spine` writes it
/// without placing it on the timeline or drawing its renders.
pub fn spine(b: &Bundle) -> scaena_core::spine::SpineProjection {
    scaena_core::spine::projection(&b.deck, None, scaena_core::spine::Pace::default())
}

/// The deck's spine replaced by `spine`, as a patch: checked, refused if it makes the deck
/// invalid, with its lint delta.
pub fn spine_update(b: &Bundle, spine: Value, dry_run: bool) -> Result<Patched, OpsError> {
    crate::patch::patch_as(b, &replace(spine), dry_run, Some("spine_update"))
}

/// [`spine_update`] with nothing written, as [`crate::patch::patching`].
pub fn spine_updating(b: &Bundle, spine: Value) -> Result<(Patched, Option<Write>), OpsError> {
    crate::patch::patching(b, &replace(spine), Some("spine_update"))
}

/// The patch that replaces the deck's spine with `spine`.
fn replace(spine: Value) -> Value {
    json!([{ "op": "add", "path": "/spine", "value": spine }])
}
