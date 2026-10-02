//! Export a projection (SPEC §10): the spine now; PDF (PLAN 1.20), frames and video (1.21),
//! and single-file HTML (2.5) name the tasks that build them.

use crate::{Bundle, OpsError};
use scaena_export::Format;

/// The bundle's deck exported as `format`: the spine as JSON. `states` picks the states a
/// frame export draws; the spine is the whole spine and takes none.
pub fn export(b: &Bundle, format: &str, states: Option<&[String]>) -> Result<serde_json::Value, OpsError> {
    let parsed: Format = format.parse().map_err(OpsError::new)?;
    let plan = match parsed {
        Format::Spine if states.is_some() => {
            return Err(OpsError::new(
                "--states picks the frames of png, pdf, svg, mp4, webm, and html; spine is the whole spine",
            ));
        }
        Format::Spine => return Ok(scaena_export::spine_json(&b.deck)),
        Format::Pdf => "1.20",
        Format::Html => "2.5",
        _ => "1.21",
    };
    Err(OpsError::not_built(
        format!("`export --format {format}` is not implemented yet — see docs/PLAN.md task {plan}"),
        plan,
    ))
}
