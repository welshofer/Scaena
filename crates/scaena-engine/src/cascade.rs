//! The cascade (SPEC §3.6), later wins: the theme's role → the node's `style` → the
//! state's props → `overrides`.
//!
//! Tracking (`scaena-core::tracking`) has already merged each node's defaults with every
//! state's delta. The cascade merges the deck's `overrides` on top of that, in every state,
//! the same way a delta merges, and turns a text node's role and style into the look it is
//! set in. Overrides are where pixel values are theme-legal, so they are counted
//! ([`Deck::overridden`]): a node with any is not theme-safe.

use crate::EngineError;
use crate::theme::{TextRole, Theme};
use scaena_core::document::{Deck, Props};
use scaena_core::model::values::TextStyle;
use scaena_core::tracking::{Snapshot, merge_props};
use serde_json::Value;

/// `snap` as it is drawn: each node's props with the deck's overrides for it merged on top.
pub fn with_overrides(deck: &Deck, snap: &Snapshot) -> Snapshot {
    let mut snap = snap.clone();
    for (id, props) in snap.nodes.iter_mut() {
        if let Some(over) = deck.overrides.get(id) {
            merge_props(props, over);
        }
    }
    snap
}

/// A text node's look as the cascade resolved it: what `scaena inspect --resolved` shows.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Look {
    /// The role it is set in: its own, or its slot's.
    pub role: String,
    /// The family's name, as its font names it.
    pub family: String,
    pub size: f64,
    pub weight: f64,
    pub leading: f64,
    pub tracking: f64,
    /// The color as the cascade names it, and as sRGB.
    pub color: String,
    pub hex: String,
}

/// A text node's look: its role (or the slot's) after its `style`.
pub fn look(theme: &Theme, props: &Props, slot_role: Option<&str>) -> Result<Look, EngineError> {
    let role = node_role(theme, props, slot_role)?;
    let name = props.get("role").and_then(Value::as_str).or(slot_role).unwrap_or_default().to_string();
    let family = theme.families().get(&role.family).map_or(role.family.clone(), |f| f.family.clone());
    let color = role.color.clone().unwrap_or_else(|| "onSurface".into());
    let hex = theme.color(&color)?.to_hex();
    // The shortest decimal that is the same f32: `0.92`, as the theme wrote it.
    let decimal = |v: f32| format!("{v}").parse::<f64>().unwrap_or(f64::from(v));
    Ok(Look {
        role: name,
        family,
        size: decimal(role.size),
        weight: decimal(role.weight),
        leading: decimal(role.leading),
        tracking: decimal(role.tracking),
        color,
        hex,
    })
}

/// What a node's props set it in: its role (or the slot's), refined by its `style`.
pub fn node_role(theme: &Theme, props: &Props, slot_role: Option<&str>) -> Result<TextRole, EngineError> {
    let name = props
        .get("role")
        .and_then(Value::as_str)
        .or(slot_role)
        .ok_or_else(|| EngineError::Layout("text node has no `role` and its slot gives none".into()))?;
    let mut role = theme.text_role(name)?;
    refine(&mut role, style(props.get("style"))?.as_ref());
    Ok(role)
}

/// What one run is set in: in its own role, else in the node's (style and all), then the
/// run's `style` over that.
pub fn run_role(theme: &Theme, node: &TextRole, run: &Value) -> Result<TextRole, EngineError> {
    let mut role = match run.get("role").and_then(Value::as_str) {
        Some(name) => theme.text_role(name)?,
        None => node.clone(),
    };
    refine(&mut role, style(run.get("style"))?.as_ref());
    Ok(role)
}

/// `style`'s properties in place of `role`'s.
pub fn refine(role: &mut TextRole, style: Option<&TextStyle>) {
    let Some(s) = style else { return };
    if let Some(family) = &s.family {
        role.family = family.clone();
    }
    if let Some(size) = s.size {
        role.size = size as f32;
    }
    if let Some(weight) = s.weight {
        role.weight = f32::from(weight);
    }
    if let Some(italic) = s.italic {
        role.italic = italic;
    }
    if let Some(leading) = s.leading {
        role.leading = leading as f32;
    }
    if let Some(tracking) = s.tracking {
        role.tracking = tracking as f32;
    }
    if let Some(opsz) = s.opsz {
        role.opsz = Some(opsz as f32);
    }
    if let Some(case) = s.case {
        role.case = Some(case);
    }
    if let Some(color) = &s.color {
        role.color = Some(color.0.clone());
    }
}

fn style(v: Option<&Value>) -> Result<Option<TextStyle>, EngineError> {
    v.cloned().map(serde_json::from_value).transpose().map_err(|e| EngineError::Layout(format!("`style`: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");

    fn deck(overrides: Value) -> Deck {
        let mut deck: Value = serde_json::from_str(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        deck["overrides"] = overrides;
        serde_json::from_value(deck).unwrap()
    }

    #[test]
    fn overrides_win_in_every_state_and_merge_like_a_delta() {
        let deck = deck(json!({ "title": { "style": { "size": 96 }, "semantic": null } }));
        let snaps = scaena_core::resolve_states(&deck).unwrap();
        for snap in &snaps {
            let Some(before) = snap.nodes.get("title") else { continue };
            let after = &with_overrides(&deck, snap).nodes["title"];
            assert_eq!(after["style"], json!({ "size": 96 }), "{}", snap.state_id);
            assert!(before.contains_key("semantic") && !after.contains_key("semantic"), "null deletes");
            assert_eq!(after["text"], before["text"], "the rest tracks as before");
        }
        assert_eq!(deck.overridden("title"), ["/style/size", "/semantic"]);
        assert!(deck.overridden("rev").is_empty());
    }

    #[test]
    fn a_role_is_refined_by_the_node_and_then_the_run() {
        let theme = Theme::from_json(DUSK).unwrap();
        let props: Props = serde_json::from_value(json!({
            "role": "body", "style": { "weight": 700, "color": "accent", "tracking": -0.01 }
        }))
        .unwrap();
        let node = node_role(&theme, &props, None).unwrap();
        let body = theme.text_role("body").unwrap();
        assert_eq!((node.weight, node.color.as_deref(), node.tracking), (700.0, Some("accent"), -0.01));
        assert_eq!((node.size, &node.family), (body.size, &body.family), "what style does not name stays the role's");

        // A run in the node's role keeps the node's style under its own.
        let run = run_role(&theme, &node, &json!({ "text": "x", "style": { "case": "upper" } })).unwrap();
        assert_eq!((run.weight, run.case), (700.0, Some(scaena_core::model::theme::Case::Upper)));
        // A run in its own role starts from that role.
        let run = run_role(&theme, &node, &json!({ "text": "4.2", "role": "numeral" })).unwrap();
        assert_eq!(run, theme.text_role("numeral").unwrap());
        // A slot's role stands in when the node names none.
        let props: Props = serde_json::from_value(json!({ "text": "x" })).unwrap();
        assert_eq!(node_role(&theme, &props, Some("caption")).unwrap(), theme.text_role("caption").unwrap());
        assert!(node_role(&theme, &props, None).is_err());
    }
}
