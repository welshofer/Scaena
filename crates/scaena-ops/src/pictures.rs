//! A picture the person hands an agent put on a slide (lift-up 10-24b FUNC-5): a logo, a product
//! shot, a chart drawn elsewhere. The PNG or JPEG joins the bundle under `assets/`, named by its
//! content, and goes in one patch by the bundle's author, refused as a patch is, as a map does
//! ([`crate::maps`]): in the image named, which shows it in place of what it showed, else in the
//! slide's first empty place for a picture, else as a new image at the box asked, entering in the
//! state and kept to its slide (PLAN 3.24). A picture past [`MAX_IMAGE_SIDE`] a side is refused
//! here: the client makes it smaller first, as the browser and the Mac do (SPEC §3.3).

use crate::lint::Write;
use crate::patch::Patched;
use crate::{Bundle, Context as _, OpsError};
use scaena_core::displaylist::MAX_IMAGE_SIDE;
use scaena_core::document::NodeType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Where a picture goes, and what it shows.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Picture {
    /// The state it goes in; the first, where none is named.
    #[serde(default)]
    pub state: Option<String>,
    /// An image it goes in, which shows it in place of what it showed.
    #[serde(default)]
    pub into: Option<String>,
    /// The box a new image takes, `[x, y, width, height]` in canvas units, where it goes in no
    /// image and the slide has no empty place for a picture.
    #[serde(default)]
    pub at: Option<[f32; 4]>,
    /// What it shows, for someone who cannot see it: the image's description (`alt`).
    #[serde(default)]
    pub alt: Option<String>,
}

/// A picture put on a slide, or refused.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Added {
    /// The image that shows it.
    pub node: String,
    /// Its path in the bundle.
    pub src: String,
    /// What it did, as an editor says it.
    pub said: String,
    #[serde(flatten)]
    pub patched: Patched,
}

/// [`add`] with nothing written: what it puts where, and what to write, unless it is refused.
pub fn adding(b: &Bundle, bytes: &[u8], picture: &Picture) -> Result<(Added, Option<Write>), OpsError> {
    let deck = &b.deck;
    let seen = scaena_engine::images::Picture::read(bytes).map_err(|e| OpsError::new(format!("the picture: {e}")))?;
    if seen.width.max(seen.height) > MAX_IMAGE_SIDE {
        return Err(OpsError::new(format!(
            "the picture is {} × {} px; a picture is at most {MAX_IMAGE_SIDE} px a side: make it smaller first",
            seen.width, seen.height
        )));
    }
    let ext = if scaena_core::jpeg::is_jpeg(bytes) { "jpg" } else { "png" };
    let hex: String = Sha256::digest(bytes).iter().map(|x| format!("{x:02x}")).collect();
    let src = format!("assets/{hex}.{ext}");
    let state = match &picture.state {
        Some(s) => deck.states.iter().find(|x| &x.id == s).ok_or_else(|| OpsError::new(format!("no state `{s}`")))?,
        None => deck.states.first().ok_or_else(|| OpsError::new("the deck has no state to put a picture in"))?,
    }
    .id
    .clone();
    let alt = picture.alt.as_deref().map(str::trim).filter(|a| !a.is_empty());
    let theme = crate::theme(b)?;
    let (node, ops, said) = if let Some(node) = &picture.into {
        let n = deck.nodes.get(node).ok_or_else(|| OpsError::new(format!("the deck has no node `{node}`")))?;
        if n.node_type != NodeType::Image {
            return Err(OpsError::new(format!("`{node}` is no image: a picture goes in an image")));
        }
        let shown = (crate::maps::shown_props(b, Some(&state), node)?)
            .ok_or_else(|| OpsError::new(format!("`{node}` is not on screen in `{state}`")))?;
        let choose = |prop: &str, value: Value| json!({ "op": "choose", "node": node, "prop": prop, "value": value, "state": state });
        let mut ops = vec![choose("src", json!(src))];
        // The whole of the new picture, framed for its box: what framed the old one goes, and so
        // does a fetched picture's record and pin, which said where the old one came from.
        for prop in ["crop", "focal", "fetched", "pin"] {
            if shown.get(prop).is_some_and(|v| !v.is_null()) {
                ops.push(choose(prop, Value::Null));
            }
        }
        if let Some(alt) = alt {
            ops.push(choose("alt", json!(alt)));
        }
        (node.clone(), ops, "Picture put in the image".to_string())
    } else {
        let waiting = match picture.at {
            None => crate::states::waiting(deck, &theme, &state, None)?.into_iter().find(|w| w.picture),
            Some(_) => None,
        };
        let at = match (&waiting, picture.at) {
            (Some(place), _) => json!({ "in": place.slot }),
            (None, Some(rect)) if rect[2] > 0.0 && rect[3] > 0.0 => json!({ "rect": rect.map(|v| v.round()) }),
            (None, Some(_)) => return Err(OpsError::new("a picture's box needs room in it")),
            (None, None) => {
                return Err(OpsError::new(format!(
                    "`{state}` has no empty place for a picture: say which image it goes in, or the box a new one takes"
                )));
            }
        };
        let id = crate::maps::fresh(deck, "picture");
        let mut node = json!({ "type": "image", "src": src, "at": at });
        if let Some(alt) = alt {
            node["alt"] = json!(alt);
        }
        let parsed: scaena_core::document::Node = serde_json::from_value(node.clone()).context("the picture")?;
        let mut ops = vec![json!({ "op": "add_node", "id": id, "node": node, "state": state })];
        for leave in crate::inspect::leaving(deck, &state, &[(id.clone(), parsed)]) {
            ops.push(serde_json::to_value(leave).context("a patch")?);
        }
        (id, ops, "Picture added".to_string())
    };
    let mut files = BTreeMap::new();
    files.insert(src.clone(), bytes.to_vec());
    let (patched, write) = crate::patch::patching_with(b, &Value::Array(ops), Some(&said), files)?;
    Ok((Added { node, src, said, patched }, write))
}

/// The PNG or JPEG `bytes` made no larger than [`MAX_IMAGE_SIDE`] a side, as the browser and the
/// Mac make a picture too large (SPEC §3.3): a PNG as a PNG with its transparency, a JPEG as a
/// JPEG turned upright by its orientation; one that fits as it is.
#[cfg(feature = "smaller")]
pub fn smaller(bytes: Vec<u8>) -> Result<Vec<u8>, OpsError> {
    use image::ImageDecoder as _;
    let seen = scaena_engine::images::Picture::read(&bytes).map_err(|e| OpsError::new(format!("the picture: {e}")))?;
    if seen.fits() {
        return Ok(bytes);
    }
    let bad = |e: image::ImageError| OpsError::new(format!("the picture could not be made smaller: {e}"));
    let jpeg = scaena_core::jpeg::is_jpeg(&bytes);
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().context("the picture")?;
    let mut decoder = reader.into_decoder().map_err(bad)?;
    let orientation = decoder.orientation().map_err(bad)?;
    let mut picture = image::DynamicImage::from_decoder(decoder).map_err(bad)?;
    picture.apply_orientation(orientation);
    let picture = picture.resize(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE, image::imageops::FilterType::Triangle);
    let mut out = std::io::Cursor::new(Vec::new());
    if jpeg {
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
        picture.to_rgb8().write_with_encoder(encoder).map_err(bad)?;
    } else {
        picture.write_to(&mut out, image::ImageFormat::Png).map_err(bad)?;
    }
    Ok(out.into_inner())
}

/// A picture, the PNG or JPEG `bytes`, put where `picture` says, written into the bundle: one
/// patch, by the bundle's author, refused as a patch is.
pub fn add(b: &Bundle, bytes: &[u8], picture: &Picture) -> Result<Added, OpsError> {
    let (added, write) = adding(b, bytes, picture)?;
    if let Some(write) = write {
        crate::lint::write(b, write)?;
    }
    Ok(added)
}
