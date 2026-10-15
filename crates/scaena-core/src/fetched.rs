//! What an object fetched from outside the deck records (ADR-0030, PLAN O.1a): where it came from,
//! when, what was asked, and the credit the service's terms ask to be shown with it.
//!
//! An image takes `fetched` (deck format 0.31): a map's picture, fetched once, with the user's own
//! key, and kept in the bundle as the image's file. Render never fetches; fetching again is an edit.
//! The record holds no key, account, or URL that holds one.
//!
//! [`SERVICES`] are the services by Scaena's name for each, with what each asks to be credited.
//! Validation refuses a service this build does not know, an `at` that is no instant, and an empty
//! credit where the service asks for one (E106, [`problems`]).

use crate::model::values::Point;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where an object came from (ADR-0030, decision 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fetched {
    /// The service, by Scaena's name for it: one of [`SERVICES`].
    pub from: String,
    /// When its answer came, an instant in UTC (RFC 3339), by the clock of the client that asked.
    pub at: String,
    /// What the person asked for, as typed: an address, a place, or `lat, lon`.
    pub asked: String,
    /// What the service's terms ask shown with it, as runs, each linked where it says.
    pub credit: Vec<CreditRun>,
    /// The data's license, where the service names one: `ODbL-1.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// A map's view, as it was asked (PLAN O.1b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<MapAsked>,
}

/// A run of a credit: its words, and where they link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreditRun {
    pub text: String,
    /// A web address the words link to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = "^https?://[^\\s]+$"))]
    pub link: Option<String>,
}

/// A map's picture as it was asked (ADR-0030, decisions 11–12): the place, the view, and the size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapAsked {
    /// The place the pin marks, as the geocoder named it, or the coordinates typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    /// The middle of the picture.
    pub center: LatLon,
    /// The service's zoom, 1 to 20: each step doubles the scale.
    #[schemars(range(min = 0, max = 22))]
    pub zoom: f64,
    /// The service's style, by its name: `positron`, `dark-matter`, or `osm-bright`.
    pub style: String,
    /// The picture's size as asked, in the service's points: the box in canvas units.
    pub size: [u32; 2],
    /// Pixels to a point: 2 asks for a picture twice as dense.
    #[schemars(range(min = 1, max = 4))]
    pub scale: u32,
}

/// A place a map marks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Place {
    /// Its name, as the geocoder gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[schemars(range(min = -90, max = 90))]
    pub lat: f64,
    #[schemars(range(min = -180, max = 180))]
    pub lon: f64,
}

/// A point on the earth, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LatLon {
    #[schemars(range(min = -90, max = 90))]
    pub lat: f64,
    #[schemars(range(min = -180, max = 180))]
    pub lon: f64,
}

/// A pin the engine draws on an image (ADR-0030, decision 13): any image may take one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    /// Where its tip stands, [x, y] in fractions of the picture, as `crop` is.
    pub at: Point,
    /// Its label, set in the theme's `label` role beside it; none, or `""`, gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A theme color: the accent by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Where an image's credit is drawn (ADR-0030, decision 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CreditPlace {
    /// On a card in the picture's bottom-left corner (the default).
    #[default]
    Inside,
    /// As a line under the picture, inside the node's box.
    Below,
}

/// The `cell` of the layer a picture's credit is drawn in, inside the picture's own (ADR-0030):
/// what a tagged PDF reads as a paragraph after the figure.
pub const CREDIT_CELL: [u32; 2] = [0, 1];

/// A service an object may be fetched from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Service {
    /// Scaena's name for it, as `fetched.from` writes it.
    pub name: &'static str,
    /// Its name as a person reads it.
    pub title: &'static str,
    /// Whether its terms ask for a credit shown with what it gave.
    pub credited: bool,
}

/// The services this build knows.
pub const SERVICES: &[Service] = &[Service { name: "geoapify", title: "Geoapify", credited: true }];

/// The service `name`, if this build knows it.
pub fn service(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.name == name)
}

/// A record's credit, its runs end to end: what a reader hears.
pub fn said(credit: &[CreditRun]) -> String {
    credit.iter().map(|r| r.text.as_str()).collect()
}

/// The date of instant `at` (`YYYY-MM-DD`), if it is one.
pub fn date(at: &str) -> Option<&str> {
    instant(at).then(|| &at[..10])
}

/// Whether `at` is an instant as RFC 3339 writes one: a date, `T`, a time to the second with any
/// fraction, and `Z` or an offset.
pub fn instant(at: &str) -> bool {
    let b = at.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r.clone()).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let number = |r: std::ops::Range<usize>| at.get(r).and_then(|s| s.parse::<u32>().ok()).unwrap_or(99);
    if b.len() < 20 || !(digits(0..4) && digits(5..7) && digits(8..10) && digits(11..13) && digits(14..16)) {
        return false;
    }
    if !digits(17..19) || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't') || b[13] != b':' {
        return false;
    }
    if b[16] != b':' {
        return false;
    }
    let (month, day, hour, minute, second) = (number(5..7), number(8..10), number(11..13), number(14..16), number(17..19));
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match &b[i..] {
        [b'Z' | b'z'] => true,
        [b'+' | b'-', h1, h2, b':', m1, m2] => {
            [h1, h2, m1, m2].iter().all(|d| d.is_ascii_digit()) && number(i + 1..i + 3) <= 23 && number(i + 4..i + 6) <= 59
        }
        _ => false,
    }
}

/// What is wrong with a `fetched` record as written, each with the pointer below the record it
/// is at: a service this build does not know, an `at` that is no instant, an empty credit where
/// the service asks for one (E106). A record the schema refuses is the schema's.
pub fn problems(record: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(fetched) = serde_json::from_value::<Fetched>(record.clone()) else { return out };
    let known = service(&fetched.from);
    if known.is_none() {
        let names: Vec<&str> = SERVICES.iter().map(|s| s.name).collect();
        out.push(("/from".into(), format!("`{}` is not a service this build knows: {}", fetched.from, names.join(", "))));
    }
    if !instant(&fetched.at) {
        out.push(("/at".into(), format!("`{}` is not an instant: write it as 2026-10-15T20:12:44Z", fetched.at)));
    }
    if known.is_some_and(|s| s.credited) && said(&fetched.credit).trim().is_empty() {
        let title = known.map_or("", |s| s.title);
        out.push(("/credit".into(), format!("{title}'s terms ask for a credit, and this one says nothing")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_instant_is_written_as_rfc_3339_writes_one() {
        for at in ["2026-10-15T20:12:44Z", "2026-10-15T20:12:44.123Z", "2026-10-15T20:12:44+02:00", "2026-10-15t20:12:44z"] {
            assert!(instant(at), "{at}");
        }
        for at in [
            "2026-10-15",
            "2026-10-15 20:12:44Z",
            "2026-10-15T20:12:44",
            "2026-13-15T20:12:44Z",
            "2026-10-15T24:00:00Z",
            "2026-10-15T20:12:44.Z",
            "2026-10-15T20:12:44+2:00",
            "yesterday",
        ] {
            assert!(!instant(at), "{at}");
        }
        assert_eq!(date("2026-10-15T20:12:44Z"), Some("2026-10-15"));
        assert_eq!(date("now"), None);
        assert_eq!(crate::words::day("2026-10-15"), "15 October 2026");
        assert_eq!(crate::words::day("2026-01-05"), "5 January 2026");
        assert_eq!(crate::words::day("someday"), "someday");
    }

    #[test]
    fn a_record_says_what_is_wrong_with_it() {
        let good = json!({
            "from": "geoapify", "at": "2026-10-15T20:12:44Z", "asked": "Paris",
            "credit": [{ "text": "© OpenStreetMap contributors", "link": "https://www.openstreetmap.org/copyright" }]
        });
        assert!(problems(&good).is_empty());
        let mut bad = good.clone();
        bad["from"] = json!("mapquest");
        bad["at"] = json!("today");
        assert_eq!(problems(&bad).iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["/from", "/at"]);
        let mut silent = good;
        silent["credit"] = json!([{ "text": " " }]);
        let found = problems(&silent);
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("Geoapify's terms ask for a credit"), "{found:?}");
    }
}
