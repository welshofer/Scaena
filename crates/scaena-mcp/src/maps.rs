//! A map from Geoapify put on a slide (ADR-0030, PLAN O.1b), for `scaena map` and the MCP
//! server's `map_fetch`: where it goes and the box it is fetched for (`scaena_ops::maps`), the
//! plan's requests made over the transport the client gives (`scaena_objects::map`), and what was
//! fetched put there in one patch. The server links no HTTP client: the CLI hands it its own.

use scaena_core::fetched::LatLon;
use scaena_objects::map::{Ask, MapPlan, Show, Style, View};
use scaena_objects::{Found, ObjectError, Ran, Transport};
use scaena_ops::maps::{Aim, Fetch, Placed, Target};
use scaena_ops::{Bundle, OpsError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a map is asked for: a place, or a map fetched again.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapFetch {
    /// The bundle's path: a directory, a `.scaena` zip, or a `deck.json`.
    pub bundle: String,
    /// Where: an address, a place, or `lat, lon`. Not with `refresh`.
    #[serde(default)]
    pub place: Option<String>,
    /// An image fetched before, fetched again: the same place and view, or `zoom` or `center`.
    #[serde(default)]
    pub refresh: Option<String>,
    /// The state it goes in: the first, where none is named.
    #[serde(default)]
    pub state: Option<String>,
    /// An image it goes in, in place of what it shows.
    #[serde(default)]
    pub into: Option<String>,
    /// The box a new image takes, `[x, y, width, height]` in canvas units, where the slide has no
    /// empty place for a picture.
    #[serde(default)]
    pub at: Option<[f32; 4]>,
    /// What the box spans across: `block` (about 500 m), `neighborhood` (2 km, the default),
    /// `city` (20 km), or `region` (200 km).
    #[serde(default)]
    pub show: Option<String>,
    /// The service's zoom, 1 to 20, in place of `show`.
    #[serde(default)]
    pub zoom: Option<f64>,
    /// The pin's label: the place's name where none is given, none where it is empty.
    #[serde(default)]
    pub label: Option<String>,
    /// `light` or `dark`, as the theme's surface asks where none is given, or `bright`.
    #[serde(default)]
    pub style: Option<String>,
    /// Which of several places the name matches, from 1, as `places` lists them.
    #[serde(default)]
    pub pick: Option<usize>,
    /// Only list the places the name matches, and write nothing.
    #[serde(default)]
    pub places: bool,
    /// A refresh's middle, `[lat, lon]`, where it is not where the map was centered.
    #[serde(default)]
    pub center: Option<[f64; 2]>,
}

/// What `map_fetch` did: the map put on the slide, or the places to pick from.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MapFetched {
    /// Where it goes, and the box it was fetched for; none where only places were listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// The map put there: one patch, or refused, saying why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placed: Option<Placed>,
    /// The places the name matched, where it matched several and none was picked, or where they
    /// were asked for: ask again with `pick`, from 1.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<Found>,
}

/// What `a` asks for, fetched over `transport` and put on the slide in `b`, written unless refused,
/// its record saying when by the transport's clock. Without a Geoapify key, nothing is asked of it.
pub fn fetch(b: &Bundle, a: &MapFetch, transport: &dyn Transport) -> Result<MapFetched, OpsError> {
    let service = scaena_objects::map::SERVICE;
    if !transport.keyed(service) {
        return Err(OpsError::new(
            "no Geoapify key: set GEOAPIFY_API_KEY to a key of your own, from geoapify.com (its Free plan takes none \
             of Scaena's)",
        ));
    }
    let view = match (a.zoom, a.show.as_deref()) {
        (Some(z), _) => View::Zoom(z),
        (None, Some(name)) => View::Show(
            Show::named(name)
                .ok_or_else(|| OpsError::new(format!("show `{name}`: block, neighborhood, city, or region")))?,
        ),
        (None, None) => View::Show(Show::Neighborhood),
    };
    let style =
        |name: &str| Style::named(name).ok_or_else(|| OpsError::new(format!("style `{name}`: light, dark, or bright")));
    let (target, ask, label) = match (&a.refresh, &a.place) {
        (Some(_), Some(_)) => return Err(OpsError::new("a refresh asks for what its map asked: name no place")),
        (Some(node), None) => {
            let center = a.center.map(|[lat, lon]| LatLon { lat, lon });
            let again = scaena_ops::maps::refreshing(b, a.state.as_deref(), node, a.zoom, center)?;
            let size = [f64::from(again.target.rect[2]), f64::from(again.target.rect[3])];
            let ask = Ask {
                place: again.asked.clone(),
                known: Some(again.place.clone()),
                center: Some(again.center),
                view: View::Zoom(again.zoom),
                style: style(a.style.as_deref().unwrap_or(&again.style))?,
                size,
                lang: again.target.lang.clone(),
                pick: None,
                list: false,
                browser: false,
            };
            let label = a.label.clone().or(again.label.clone()).or(Some(String::new()));
            (again.target, ask, label)
        }
        (None, Some(place)) if a.places => {
            let lang = b.deck.meta.as_ref().and_then(|m| m.lang.clone());
            let ask = Ask {
                place: place.clone(),
                known: None,
                center: None,
                view,
                style: Style::Light,
                size: [800.0, 450.0],
                lang,
                pick: None,
                list: true,
                browser: false,
            };
            let (mut plan, first) = MapPlan::new(ask).map_err(said)?;
            return match scaena_objects::run(&mut plan, first, transport).map_err(said)? {
                Ran::Choose(places) => Ok(MapFetched { target: None, placed: None, places }),
                Ran::Done(_) => Err(OpsError::new("listing places drew a map")),
            };
        }
        (None, Some(place)) => {
            let aim = Aim { state: a.state.clone(), into: a.into.clone(), at: a.at };
            let target = scaena_ops::maps::aiming(b, &aim)?;
            let ask = Ask {
                place: place.clone(),
                known: None,
                center: None,
                view,
                style: style(a.style.as_deref().unwrap_or(&target.style))?,
                size: [f64::from(target.rect[2]), f64::from(target.rect[3])],
                lang: target.lang.clone(),
                pick: a.pick.map(|n| n.saturating_sub(1)),
                list: false,
                browser: false,
            };
            (target, ask, a.label.clone())
        }
        (None, None) => return Err(OpsError::new("say where: an address, a place, or `lat, lon`")),
    };
    if a.pick == Some(0) {
        return Err(OpsError::new("pick a place from 1, as they are listed"));
    }
    let (mut plan, first) = MapPlan::new(ask).map_err(said)?;
    match scaena_objects::run(&mut plan, first, transport).map_err(said)? {
        Ran::Choose(places) => Ok(MapFetched { target: Some(target), placed: None, places }),
        Ran::Done(map) => {
            let fetch = Fetch { fetched: map.fetched(&transport.now()), pin: map.pin, label, png: map.png };
            let placed = scaena_ops::maps::place(b, &target, &fetch)?;
            Ok(MapFetched { target: Some(target), placed: Some(placed), places: Vec::new() })
        }
    }
}

/// A plan's stop, as an operation's: what stopped it, in the service's words where it gave them.
fn said(e: ObjectError) -> OpsError {
    OpsError::new(e.to_string())
}
