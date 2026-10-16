//! A map put on a slide (ADR-0030, PLAN O.1b): a picture fetched from a service, which the client
//! fetched by `scaena-objects`' plan, joins the bundle, named by its content, and goes in one patch
//! by its author, refused as a patch is.
//!
//! - [`aiming`] says where a map goes in a state and the box it is fetched for: the image named,
//!   else the slide's first empty place for a picture, else a new image at the box asked; and the
//!   style the theme's surface asks, and the deck's language, which the geocoder names places in.
//! - [`placing`] puts what was fetched there: in an image, its `src`, `fetched`, and `pin`, and the
//!   whole of the picture, a crop or a focal point it had taken away; or a new image, entering in
//!   the state and kept to its slide (PLAN 3.24).
//! - [`refreshing`] says what a refresh asks, from the image's record: the same place and view,
//!   or another zoom or center, for the box the image has now. Its answer goes in by [`placing`],
//!   into the image: one patch, one step to undo, both pictures in the bundle.

use crate::lint::Write;
use crate::patch::Patched;
use crate::{Bundle, Context as _, OpsError};
use scaena_core::document::NodeType;
use scaena_core::fetched::{Fetched, LatLon, Pin, Place};
use scaena_engine::FrameRequest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Where a map is asked to go.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Aim {
    /// The state it goes in; the first, where none is named.
    #[serde(default)]
    pub state: Option<String>,
    /// An image it goes in, which shows it in place of what it showed.
    #[serde(default)]
    pub into: Option<String>,
    /// The box a new image takes, `[x, y, width, height]` in canvas units, where the slide has no
    /// empty place for a picture.
    #[serde(default)]
    pub at: Option<[f32; 4]>,
}

/// Where a map goes, and what it is fetched for.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Target {
    pub state: String,
    /// The image it goes in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The empty place for a picture it fills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// The box it is drawn in, canvas units: the picture is asked for at its size.
    pub rect: [f32; 4],
    /// The picture's style, as the theme's surface asks: `light` or `dark`.
    pub style: String,
    /// The deck's language, which the geocoder names places in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// What a client fetched: the picture, its record, and where the place stands in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetch {
    /// The PNG the service drew.
    pub png: Vec<u8>,
    pub fetched: Fetched,
    /// Where the pin's tip stands, in fractions of the picture.
    pub pin: [f64; 2],
    /// The pin's label: the place's name where none is given, none where it is empty.
    pub label: Option<String>,
}

/// A map put on a slide, or refused.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Placed {
    /// The image that shows it.
    pub node: String,
    /// Its picture's path in the bundle.
    pub src: String,
    /// The place it marks.
    pub place: String,
    /// What it did, as an editor says it.
    pub said: String,
    #[serde(flatten)]
    pub patched: Patched,
}

/// What a refresh asks for: the image's place and view, or another, for its box now.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Refresh {
    pub target: Target,
    /// What was asked first, as its record says.
    pub asked: String,
    pub place: Place,
    pub center: LatLon,
    pub zoom: f64,
    /// The service's style, as its record says.
    pub style: String,
    /// The pin's label now, which a refresh keeps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Where a map asked for by `aim` goes in `b`'s deck, and the box it is fetched for.
pub fn aiming(b: &Bundle, aim: &Aim) -> Result<Target, OpsError> {
    let deck = &b.deck;
    let state = match &aim.state {
        Some(s) => deck.states.iter().find(|x| &x.id == s).ok_or_else(|| OpsError::new(format!("no state `{s}`")))?,
        None => deck.states.first().ok_or_else(|| OpsError::new("the deck has no state to put a map in"))?,
    }
    .id
    .clone();
    let theme = crate::theme(b)?;
    let surface = theme.color("surface").context("the theme's surface")?;
    let style = if luminance(surface.0) < 0.18 { "dark" } else { "light" }.to_string();
    let lang = deck.meta.as_ref().and_then(|m| m.lang.clone());
    if let Some(node) = &aim.into {
        let n = deck.nodes.get(node).ok_or_else(|| OpsError::new(format!("the deck has no node `{node}`")))?;
        if n.node_type != NodeType::Image {
            return Err(OpsError::new(format!("`{node}` is no image: a map goes in an image")));
        }
        let rect = box_of(b, &theme, &state, node)?
            .ok_or_else(|| OpsError::new(format!("`{node}` is not on screen in `{state}`")))?;
        return Ok(Target { state, node: Some(node.clone()), slot: None, rect, style, lang });
    }
    if aim.at.is_none()
        && let Some(place) = crate::states::waiting(deck, &theme, &state, None)?.into_iter().find(|w| w.picture)
    {
        return Ok(Target { state, node: None, slot: Some(place.slot), rect: place.rect, style, lang });
    }
    let rect = aim.at.ok_or_else(|| {
        OpsError::new(format!(
            "`{state}` has no empty place for a picture: say which image it goes in, or the box a new one takes"
        ))
    })?;
    if !(rect[2] > 0.0 && rect[3] > 0.0) {
        return Err(OpsError::new("a map's box needs room in it"));
    }
    Ok(Target { state, node: None, slot: None, rect, style, lang })
}

/// What a refresh of image `node` in `state` asks for: the place its record names, at its view, or
/// at `zoom` or centered on `center` where they are given, for the box it has now.
pub fn refreshing(
    b: &Bundle,
    state: Option<&str>,
    node: &str,
    zoom: Option<f64>,
    center: Option<LatLon>,
) -> Result<Refresh, OpsError> {
    let deck = &b.deck;
    let props = &deck.nodes.get(node).ok_or_else(|| OpsError::new(format!("the deck has no node `{node}`")))?.props;
    // As the state shows it: its record may be written there.
    let shown = shown_props(b, state, node)?;
    let props = shown.as_ref().unwrap_or(props);
    let record: Fetched = props
        .get("fetched")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .context("its record")?
        .ok_or_else(|| OpsError::new(format!("`{node}` was not fetched: there is nothing to fetch again")))?;
    let map = record.map.clone().ok_or_else(|| OpsError::new(format!("`{node}` is no map")))?;
    let place = map.place.clone().unwrap_or(Place { name: None, lat: map.center.lat, lon: map.center.lon });
    let label = props.get("pin").and_then(|p| p.get("label")).and_then(Value::as_str).map(String::from);
    let aim = Aim { state: state.map(String::from), into: Some(node.into()), at: None };
    let target = aiming(b, &aim)?;
    Ok(Refresh {
        target,
        asked: record.asked,
        place,
        center: center.unwrap_or(map.center),
        zoom: zoom.unwrap_or(map.zoom),
        style: map.style,
        label,
    })
}

/// [`place`] with nothing written: what it puts where, and what to write, unless it is refused.
pub fn placing(b: &Bundle, target: &Target, fetch: &Fetch) -> Result<(Placed, Option<Write>), OpsError> {
    let deck = &b.deck;
    scaena_engine::images::ImageInfo::read(&fetch.png).map_err(|e| OpsError::new(format!("the map's picture: {e}")))?;
    let hex: String = Sha256::digest(&fetch.png).iter().map(|x| format!("{x:02x}")).collect();
    let src = format!("assets/{hex}.png");
    let place = fetch
        .fetched
        .map
        .as_ref()
        .and_then(|m| m.place.as_ref())
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| fetch.fetched.asked.clone());
    let label = match &fetch.label {
        Some(l) if l.trim().is_empty() => None,
        Some(l) => Some(l.clone()),
        None => Some(place.clone()),
    };
    let pin = Pin { at: fetch.pin, label, color: None };
    let fetched = serde_json::to_value(&fetch.fetched).context("its record")?;
    let pin = serde_json::to_value(&pin).context("its pin")?;
    let service = scaena_core::fetched::service(&fetch.fetched.from).map_or(fetch.fetched.from.as_str(), |s| s.title);
    let state = target.state.as_str();
    let (node, ops, said) = match &target.node {
        Some(node) => {
            let shown = shown_props(b, Some(state), node)?.unwrap_or_default();
            let choose = |prop: &str, value: Value| json!({ "op": "choose", "node": node, "prop": prop, "value": value, "state": state });
            let mut ops = vec![choose("src", json!(src)), choose("fetched", fetched), choose("pin", pin)];
            // The whole of the new picture, framed for its box: what framed the old one goes.
            for prop in ["crop", "focal"] {
                if shown.get(prop).is_some_and(|v| !v.is_null()) {
                    ops.push(choose(prop, Value::Null));
                }
            }
            (node.clone(), ops, format!("Map of {place} from {service} put in the picture"))
        }
        None => {
            let id = fresh(deck, "map");
            let at = match &target.slot {
                Some(slot) => json!({ "in": slot }),
                None => json!({ "rect": target.rect.map(|v| v.round()) }),
            };
            let node = json!({ "type": "image", "src": src, "fetched": fetched, "pin": pin, "at": at });
            let parsed: scaena_core::document::Node = serde_json::from_value(node.clone()).context("the map")?;
            let mut ops = vec![json!({ "op": "add_node", "id": id, "node": node, "state": state })];
            for leave in crate::inspect::leaving(deck, state, &[(id.clone(), parsed)]) {
                ops.push(serde_json::to_value(leave).context("a patch")?);
            }
            (id, ops, format!("Map of {place} from {service} added"))
        }
    };
    let mut files = BTreeMap::new();
    files.insert(src.clone(), fetch.png.clone());
    let (patched, write) = crate::patch::patching_with(b, &Value::Array(ops), Some(&said), files)?;
    Ok((Placed { node, src, place, said, patched }, write))
}

/// What was fetched put where `target` says, written into the bundle: one patch, by the bundle's
/// author, refused as a patch is.
pub fn place(b: &Bundle, target: &Target, fetch: &Fetch) -> Result<Placed, OpsError> {
    let (placed, write) = placing(b, target, fetch)?;
    if let Some(write) = write {
        crate::lint::write(b, write)?;
    }
    Ok(placed)
}

/// Node `node`'s props as `state` shows them, its overrides last; none where it shows none.
fn shown_props(b: &Bundle, state: Option<&str>, node: &str) -> Result<Option<scaena_core::document::Props>, OpsError> {
    let snaps = scaena_core::resolve_states(&b.deck).context("tracking")?;
    let snap = match state {
        Some(s) => snaps.iter().find(|x| x.state_id == s),
        None => snaps.iter().find(|x| x.nodes.contains_key(node)),
    };
    let Some(snap) = snap else { return Ok(None) };
    let shown = scaena_engine::cascade::with_overrides(&b.deck, snap);
    Ok(shown.nodes.get(node).cloned())
}

/// Node `node`'s box at rest in `state`, canvas units.
fn box_of(
    b: &Bundle,
    theme: &scaena_engine::theme::Theme,
    state: &str,
    node: &str,
) -> Result<Option<[f32; 4]>, OpsError> {
    let mut engine = crate::lint::engine_with(b, theme, None)?;
    let data = crate::lint::data_files(b)?;
    let req = FrameRequest { deck: &b.deck, theme, data: &data, state, t_ms: f64::INFINITY, format: None };
    let scene = engine.at_rest(&req)?;
    Ok(scene.boxes().into_iter().find(|x| x.node == node).map(|x| x.rect))
}

/// The first of `base`, `base-2`, `base-3`, … that names no node of `deck`.
fn fresh(deck: &scaena_core::Deck, base: &str) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !deck.nodes.contains_key(id))
        .expect("some number is free")
}

/// An sRGB color's relative luminance, as WCAG defines it.
fn luminance([r, g, b, _]: [u8; 4]) -> f64 {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}
