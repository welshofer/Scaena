//! A map's picture from Geoapify (ADR-0030, decisions 10–14): the place from its geocoder, then
//! the picture from its static map, centered on the place, as the box's size in canvas units at
//! twice the density, PNG, in the style the theme's surface asks.
//!
//! - **The place.** `api.geoapify.com/v1/geocode/search`, with `text`, `lang` (the deck's language
//!   as its two letters), `limit=5`, and `format=json`; skipped for coordinates typed as `lat,
//!   lon`. Several places found are offered to choose from, unless one is picked.
//! - **The picture.** `maps.geoapify.com/v1/staticmap`, with `style`, `width` and `height` (the box
//!   in canvas units, 2,048 at most), `scaleFactor=2`, `center=lonlat:`, `zoom`, and `format=png`.
//!   The key goes in `apiKey`. No marker is asked for: the pin is the engine's.
//! - **The zoom.** Show sets it from what the box spans across, worked out by `libm`, so every
//!   client asks for the same picture: a block (500 m), a neighborhood (2 km), a city (20 km), or a
//!   region (200 km).
//! - **The pin's place** in the picture is worked out by Web Mercator on Geoapify's 256-point
//!   tiles: the middle, where the picture is centered on the place.
//! - **The credit** is what Geoapify's terms and OpenStreetMap's guideline ask: OpenStreetMap's,
//!   with openstreetmap.org/copyright written out for print, OpenMapTiles', and Geoapify's, each
//!   linked; a place's own source's after them, where it is not OpenStreetMap.

use crate::{Answer, Found, Get, Key, ObjectError, Plan, Step};
use scaena_core::fetched::{CreditRun, Fetched, LatLon, MapAsked, Place};
use serde_json::Value;

/// Scaena's name for the service.
pub const SERVICE: &str = "geoapify";
const TITLE: &str = "Geoapify";
const GEOCODE: &str = "https://api.geoapify.com/v1/geocode/search";
const STATIC: &str = "https://maps.geoapify.com/v1/staticmap";
/// The most a geocoder's answer may hold.
const MOST_PLACES: u64 = 1 << 20;
/// The most a picture may hold: 4,096 px a side, as PNG.
const MOST_PICTURE: u64 = 48 << 20;
/// The longest side a picture is asked for, in points: twice that in pixels, Geoapify's most.
pub const LONGEST: f64 = 2048.0;
/// Pixels to a point.
pub const SCALE: u32 = 2;
/// The data's license: OpenStreetMap's.
pub const LICENSE: &str = "ODbL-1.0";

/// What the box spans across, as Show says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// About 500 m.
    Block,
    /// About 2 km.
    Neighborhood,
    /// About 20 km.
    City,
    /// About 200 km.
    Region,
}

impl Show {
    /// `name` as Show's choices write it.
    pub fn named(name: &str) -> Option<Show> {
        match name {
            "block" => Some(Show::Block),
            "neighborhood" | "neighbourhood" => Some(Show::Neighborhood),
            "city" => Some(Show::City),
            "region" => Some(Show::Region),
            _ => None,
        }
    }

    /// The meters it spans across.
    pub fn meters(self) -> f64 {
        match self {
            Show::Block => 500.0,
            Show::Neighborhood => 2_000.0,
            Show::City => 20_000.0,
            Show::Region => 200_000.0,
        }
    }
}

/// The picture's style, as the theme's surface asks it or a person picks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// On a light surface: Geoapify's `positron`.
    Light,
    /// On a dark surface: `dark-matter`.
    Dark,
    /// Where asked: `osm-bright`.
    Bright,
}

impl Style {
    /// `light`, `dark`, or `bright`; or Geoapify's own name for one.
    pub fn named(name: &str) -> Option<Style> {
        match name {
            "light" | "positron" => Some(Style::Light),
            "dark" | "dark-matter" => Some(Style::Dark),
            "bright" | "osm-bright" => Some(Style::Bright),
            _ => None,
        }
    }

    /// Geoapify's name for it.
    pub fn service(self) -> &'static str {
        match self {
            Style::Light => "positron",
            Style::Dark => "dark-matter",
            Style::Bright => "osm-bright",
        }
    }

    /// The style a surface of this lightness asks for (its relative luminance, 0 to 1).
    pub fn for_surface(luminance: f64) -> Style {
        if luminance < 0.18 { Style::Dark } else { Style::Light }
    }
}

/// How near the picture is: by what it shows across, or the service's zoom as asked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Show(Show),
    Zoom(f64),
}

/// What a map is asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// What the person typed: an address, a place, or `lat, lon`. A refresh asks for what its
    /// record asked.
    pub place: String,
    /// The place known already, which skips the geocoder: a refresh's, or one picked.
    pub known: Option<Place>,
    /// The picture's middle, where it is not the place's: a refresh centered elsewhere.
    pub center: Option<LatLon>,
    pub view: View,
    pub style: Style,
    /// The box in canvas units: the picture's size in points.
    pub size: [f64; 2],
    /// The deck's language (BCP 47): the geocoder names places in it.
    pub lang: Option<String>,
    /// Which of several places found: by its place in the list, from 0.
    pub pick: Option<usize>,
    /// Only the places the name matches, to choose from: no picture is asked for.
    pub list: bool,
    /// Whether the client is a browser, which sets its own User-Agent.
    pub browser: bool,
}

/// A map fetched: the picture, and all its record says but when its answer came, which the client
/// that asked knows by its clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    /// The PNG Geoapify drew.
    pub png: Vec<u8>,
    pub asked: String,
    pub credit: Vec<CreditRun>,
    pub map: MapAsked,
    /// Where the place stands in the picture, in fractions of it: the pin's `at`.
    pub pin: [f64; 2],
}

impl Map {
    /// Its record, its answer having come `at` (RFC 3339, UTC).
    pub fn fetched(&self, at: &str) -> Fetched {
        Fetched {
            from: SERVICE.into(),
            at: at.into(),
            asked: self.asked.clone(),
            credit: self.credit.clone(),
            license: Some(LICENSE.into()),
            map: Some(self.map.clone()),
        }
    }

    /// The place's name, as the geocoder gave it, or what was asked.
    pub fn place_name(&self) -> String {
        self.map.place.as_ref().and_then(|p| p.name.clone()).unwrap_or_else(|| self.asked.clone())
    }
}

/// The plan: the place, if it is not known, then the picture.
#[derive(Debug)]
pub struct MapPlan {
    ask: Ask,
    headers: Vec<(String, String)>,
    /// The places found, from the geocoder's answer, each with its source's credit.
    found: Vec<(Found, Option<CreditRun>)>,
    /// The place chosen, and its source's credit, once known.
    place: Option<(Place, Option<CreditRun>)>,
    /// The picture's middle, size in points, and zoom, once asked.
    picture: Option<(LatLon, [u32; 2], f64)>,
}

/// `lat, lon` as typed, if `text` is coordinates: two numbers, a latitude and a longitude.
pub fn coordinates(text: &str) -> Option<LatLon> {
    let (a, b) = text.split_once(',')?;
    let (lat, lon) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) && lat.is_finite() && lon.is_finite())
        .then_some(LatLon { lat, lon })
}

/// The picture's size in points for a box of `size` canvas units: as it is, its longest side held
/// to [`LONGEST`], each side at least one point.
pub fn points(size: [f64; 2]) -> [u32; 2] {
    let [w, h] = size.map(|v| if v.is_finite() { v.max(1.0) } else { 1.0 });
    let k = (LONGEST / w.max(h)).min(1.0);
    [(w * k).round().max(1.0) as u32, (h * k).round().max(1.0) as u32]
}

/// The zoom at which `width` points span `meters` at latitude `lat`, on 256-point Web Mercator
/// tiles, to two places, between 1 and 20.
pub fn zoom(width: u32, lat: f64, meters: f64) -> f64 {
    let per_point = 156_543.033_92 * libm::cos(lat.to_radians());
    let z = libm::log2(f64::from(width) * per_point / meters.max(1.0));
    (libm::round(z.clamp(1.0, 20.0) * 100.0)) / 100.0
}

/// Where `p` stands in a picture of `size` points centered on `center` at `zoom`, in fractions of
/// the picture, by Web Mercator on 256-point tiles.
pub fn spot(p: LatLon, center: LatLon, zoom: f64, size: [u32; 2]) -> [f64; 2] {
    let world = 256.0 * libm::exp2(zoom);
    let x = |lon: f64| (lon + 180.0) / 360.0 * world;
    let y = |lat: f64| {
        let s = libm::sin(lat.clamp(-85.051_128_78, 85.051_128_78).to_radians());
        (0.5 - libm::log((1.0 + s) / (1.0 - s)) / (4.0 * std::f64::consts::PI)) * world
    };
    let [w, h] = size.map(f64::from);
    let round = |v: f64| libm::round(v * 1e6) / 1e6;
    [round(0.5 + (x(p.lon) - x(center.lon)) / w), round(0.5 + (y(p.lat) - y(center.lat)) / h)]
}

/// The credit Geoapify's terms and OpenStreetMap's guideline ask, each part linked, with the
/// place's own source's after it.
pub fn credit(source: Option<&CreditRun>) -> Vec<CreditRun> {
    let run = |text: &str, link: Option<&str>| CreditRun { text: text.into(), link: link.map(String::from) };
    let mut out = vec![
        run("©\u{a0}OpenStreetMap contributors", Some("https://www.openstreetmap.org/copyright")),
        run(" · openstreetmap.org/copyright · ", None),
        run("©\u{a0}OpenMapTiles", Some("https://openmaptiles.org/")),
        run(" · ", None),
        run("Powered by Geoapify", Some("https://www.geoapify.com/")),
    ];
    if let Some(source) = source {
        out.push(run(" · ", None));
        out.push(source.clone());
    }
    out
}

impl MapPlan {
    /// The plan for `ask`, and its first request: the geocoder's, or the picture's where the place
    /// is known or typed as coordinates.
    pub fn new(ask: Ask) -> Result<(MapPlan, Get), ObjectError> {
        if !(ask.size[0] > 0.0 && ask.size[1] > 0.0) {
            return Err(ObjectError::Asked("a map needs a box with room in it".into()));
        }
        let mut headers = vec![("accept".to_string(), "application/json, image/png".to_string())];
        if !ask.browser {
            headers.push(("user-agent".to_string(), crate::user_agent()));
        }
        let typed = coordinates(&ask.place).map(|c| Place { name: None, lat: c.lat, lon: c.lon });
        let known = if ask.list { None } else { ask.known.clone().or(typed) };
        if known.is_none() && ask.place.trim().is_empty() {
            return Err(ObjectError::Asked("say where: an address, a place, or `lat, lon`".into()));
        }
        let mut plan = MapPlan { ask, headers, found: Vec::new(), place: None, picture: None };
        let get = match known {
            Some(place) => {
                plan.place = Some((place, None));
                plan.picture_get()
            }
            None => plan.places_get(),
        };
        Ok((plan, get))
    }

    fn get(&self, url: String, most: u64) -> Get {
        Get {
            url,
            headers: self.headers.clone(),
            most,
            service: SERVICE.into(),
            key: Some(Key::Query("apiKey".into())),
        }
    }

    fn places_get(&self) -> Get {
        let mut url = format!("{GEOCODE}?text={}", crate::encode(self.ask.place.trim()));
        if let Some(lang) = self.ask.lang.as_deref().and_then(two_letters) {
            url.push_str(&format!("&lang={lang}"));
        }
        url.push_str("&limit=5&format=json");
        self.get(url, MOST_PLACES)
    }

    fn picture_get(&mut self) -> Get {
        let (place, _) = self.place.as_ref().expect("a place known");
        let center = self.ask.center.unwrap_or(LatLon { lat: place.lat, lon: place.lon });
        let size = points(self.ask.size);
        let zoom = match self.ask.view {
            View::Show(show) => zoom(size[0], center.lat, show.meters()),
            View::Zoom(z) => (libm::round(z.clamp(1.0, 20.0) * 100.0)) / 100.0,
        };
        self.picture = Some((center, size, zoom));
        let url = format!(
            "{STATIC}?style={}&width={}&height={}&scaleFactor={SCALE}&center=lonlat:{},{}&zoom={}&format=png",
            self.ask.style.service(),
            size[0],
            size[1],
            degrees(center.lon),
            degrees(center.lat),
            zoom
        );
        self.get(url, MOST_PICTURE)
    }
}

/// A language tag's two letters, as the geocoder takes it: `en` of `en-US`.
fn two_letters(lang: &str) -> Option<String> {
    let first = lang.split(['-', '_']).next()?.to_ascii_lowercase();
    (first.len() == 2 && first.bytes().all(|b| b.is_ascii_lowercase())).then_some(first)
}

/// Degrees to six places, the way a URL writes them.
fn degrees(v: f64) -> String {
    let s = format!("{:.6}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// What Geoapify said in refusing, in its own words where its answer gives them.
fn refusal(status: u16, body: &[u8]) -> ObjectError {
    let said = serde_json::from_slice::<Value>(body).ok().and_then(|v| {
        let message = v.get("message").and_then(Value::as_str).map(str::trim).filter(|m| !m.is_empty());
        let error = v.get("error").and_then(Value::as_str).map(str::trim).filter(|m| !m.is_empty());
        match (error, message) {
            (Some(e), Some(m)) if e != m => Some(format!("{e}: {m}")),
            (_, Some(m)) => Some(m.to_string()),
            (Some(e), None) => Some(e.to_string()),
            _ => None,
        }
    });
    let words = said.unwrap_or_else(|| match status {
        401 | 403 => "the key was not taken".into(),
        429 => "too many requests".into(),
        _ => format!("it answered {status}"),
    });
    ObjectError::Refused { service: TITLE.into(), status, words }
}

/// A geocoder's result as a place found, and its source's credit where it is not OpenStreetMap's.
fn found(result: &Value) -> Option<(Found, Option<CreditRun>)> {
    let text = |k: &str| result.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let (lat, lon) = (result.get("lat")?.as_f64()?, result.get("lon")?.as_f64()?);
    let address = text("formatted").or(text("address_line1"))?.to_string();
    let name = text("name").or(text("address_line1")).unwrap_or(&address).to_string();
    let kind = text("result_type").map(|k| k.replace('_', " "));
    let source = result.get("datasource").and_then(|d| {
        let name = d.get("sourcename").and_then(Value::as_str).unwrap_or_default();
        if name.eq_ignore_ascii_case("openstreetmap") {
            return None;
        }
        let words = d.get("attribution").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())?;
        let link =
            d.get("url").and_then(Value::as_str).filter(|u| u.starts_with("https://") || u.starts_with("http://"));
        Some(CreditRun { text: words.to_string(), link: link.map(String::from) })
    });
    Some((Found { name, address, lat, lon, kind }, source))
}

impl Plan for MapPlan {
    type Done = Map;

    fn answer(&mut self, a: &Answer) -> Result<Step<Map>, ObjectError> {
        if a.status != 200 {
            return Err(refusal(a.status, &a.body));
        }
        if self.place.is_none() {
            let v: Value = serde_json::from_slice(&a.body)
                .map_err(|_| ObjectError::Unreadable(format!("{TITLE}'s geocoder answered with no JSON")))?;
            self.found = v.get("results").and_then(Value::as_array).into_iter().flatten().filter_map(found).collect();
            let chosen = match (self.found.len(), self.ask.pick) {
                (0, _) => {
                    return Err(ObjectError::Asked(format!(
                        "{TITLE} found no place called “{}”: try an address, a city, or `lat, lon`",
                        self.ask.place.trim()
                    )));
                }
                (n, Some(k)) if k >= n => {
                    return Err(ObjectError::Asked(format!(
                        "{TITLE} found {n} places called “{}”: pick one of 1 to {n}",
                        self.ask.place.trim()
                    )));
                }
                (_, _) if self.ask.list => {
                    return Ok(Step::Choose(self.found.iter().map(|(f, _)| f.clone()).collect()));
                }
                (_, Some(k)) => k,
                (1, None) => 0,
                (_, None) => return Ok(Step::Choose(self.found.iter().map(|(f, _)| f.clone()).collect())),
            };
            let (f, source) = self.found[chosen].clone();
            self.place = Some((Place { name: Some(f.name), lat: f.lat, lon: f.lon }, source));
            return Ok(Step::Get(self.picture_get()));
        }
        if !a.body.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(ObjectError::Unreadable(format!("{TITLE}'s static map answered with no PNG")));
        }
        let (place, source) = self.place.clone().expect("a place known");
        let (center, size, zoom) = self.picture.expect("a picture asked");
        let pin = spot(LatLon { lat: place.lat, lon: place.lon }, center, zoom, size);
        let map =
            MapAsked { place: Some(place), center, zoom, style: self.ask.style.service().into(), size, scale: SCALE };
        Ok(Step::Done(Map {
            png: a.body.clone(),
            asked: self.ask.place.trim().to_string(),
            credit: credit(source.as_ref()),
            map,
            pin,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_are_two_numbers_on_the_earth() {
        assert_eq!(coordinates("37.7955, -122.3937"), Some(LatLon { lat: 37.7955, lon: -122.3937 }));
        assert_eq!(coordinates(" 0,0 "), Some(LatLon { lat: 0.0, lon: 0.0 }));
        assert_eq!(coordinates("91, 0"), None);
        assert_eq!(coordinates("1600 Pennsylvania Ave, Washington"), None);
    }

    #[test]
    fn the_zoom_spans_what_show_says_across_the_box() {
        // 1,920 points across 2 km at the equator: log2(1920 × 156543.03392 / 2000) ≈ 17.2.
        assert_eq!(zoom(1920, 0.0, 2_000.0), 17.2);
        // Further from the equator, a point spans less ground: the zoom is lower for the same span.
        assert!(zoom(1920, 60.0, 2_000.0) < zoom(1920, 0.0, 2_000.0));
        assert_eq!(zoom(1920, 0.0, 1e12), 1.0, "held to 1");
        assert_eq!(points([3840.0, 2160.0]), [2048, 1152]);
        assert_eq!(points([400.0, 225.0]), [400, 225]);
    }

    #[test]
    fn the_place_stands_where_mercator_puts_it() {
        let c = LatLon { lat: 37.8, lon: -122.4 };
        assert_eq!(spot(c, c, 15.0, [800, 450]), [0.5, 0.5]);
        // One point east at zoom 0 is 360/256 degrees.
        let east = LatLon { lat: 0.0, lon: 360.0 / 256.0 * 100.0 };
        let [x, y] = spot(east, LatLon { lat: 0.0, lon: 0.0 }, 0.0, [400, 400]);
        assert!((x - 0.75).abs() < 1e-6 && (y - 0.5).abs() < 1e-6, "{x} {y}");
    }
}
