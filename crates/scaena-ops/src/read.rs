//! Read a deck and its spine, and replace the spine (SPEC §2.6, §3.11).

use crate::patch::{Patched, patch};
use crate::{Bundle, OpsError};
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
}

/// The bundle's deck: as JSON, or with `scn`, as `.scn`.
pub fn read(b: &Bundle, scn: bool) -> Result<Read, OpsError> {
    Ok(match scn {
        true => Read { deck: None, scn: Some(scaena_core::dsl::decompile(&b.deck)) },
        false => Read { deck: Some(serde_json::to_value(&b.deck)?), scn: None },
    })
}

/// The deck's spine: its sections and beats, each beat with its states (SPEC §3.11), as
/// `export --format spine` writes it.
pub fn spine(b: &Bundle) -> Value {
    scaena_export::spine_json(&b.deck)
}

/// The deck's spine replaced by `spine`, as a patch: checked, refused if it makes the deck
/// invalid, with its lint delta.
pub fn spine_update(b: &Bundle, spine: Value, dry_run: bool) -> Result<Patched, OpsError> {
    patch(b, &json!([{ "op": "add", "path": "/spine", "value": spine }]), dry_run)
}
