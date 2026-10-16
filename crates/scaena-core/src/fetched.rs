//! What an object fetched from outside the deck records (ADR-0030, PLAN O.1a): where it came from,
//! when, what was asked, and the credit the service's terms ask to be shown with it.
//!
//! An image takes `fetched` (deck format 0.29): a map's picture, fetched once, with the user's own
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
    /// Which of the service's terms applied, where the request itself says so: Open-Meteo's
    /// `free` service or its `customer` one (deck format 0.30).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<Terms>,
    /// The person keeps it as of its date: lint does not judge its date (W428).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
    /// Its rows were edited since they came, which the credit says, as CC BY asks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
    /// A forecast, as it was asked (PLAN O.2a): the place, the period, and the units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forecast: Option<crate::weather::Forecast>,
    /// A price series, as it was asked and as far as it runs (PLAN O.3a, deck format 0.32).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<Stock>,
}

/// A price series as it was fetched (ADR-0030, decision 22): the symbol, the range, where it
/// trades and in what, and the last price's civil time there. Its rows are `time`, `close`, and
/// `base`, in a CSV under `data/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stock {
    /// The ticker symbol, as the service names it: `AAPL`.
    #[schemars(length(min = 1))]
    pub symbol: String,
    /// How far back the series runs.
    pub range: Range,
    /// The exchange it trades on, as the service names it: `NASDAQ`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    /// The currency its prices are in, ISO 4217: `USD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = "^[A-Z]{3}$"))]
    pub currency: Option<String>,
    /// The exchange's time zone, by its IANA name, in which the rows' times are civil:
    /// `America/New_York`.
    #[schemars(length(min = 1))]
    pub zone: String,
    /// The last price's civil time in the exchange's zone: a day, `2026-10-15`, or a close at its
    /// bar's end, `2026-10-15T16:00`.
    pub through: String,
}

/// How far back a price series runs: a day, a week, a month, three months, a year, or the year
/// to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Range {
    #[serde(rename = "1d")]
    Day,
    #[serde(rename = "1w")]
    Week,
    #[serde(rename = "1m")]
    Month,
    #[serde(rename = "3m")]
    Quarter,
    #[serde(rename = "1y")]
    Year,
    #[serde(rename = "ytd")]
    YearToDate,
}

impl Range {
    /// The days its series may run behind before lint finds it old (ADR-0030, decision 25): a day
    /// for a day or a week, a week for a month, three months, or the year to date, a month for a
    /// year.
    pub fn allowed(self) -> i64 {
        match self {
            Range::Day | Range::Week => 1,
            Range::Month | Range::Quarter | Range::YearToDate => 7,
            Range::Year => 31,
        }
    }
}

/// Whether `at` is a civil time as a price series writes one: a day, `YYYY-MM-DD`, or a day and a
/// time to the minute or the second, `YYYY-MM-DDTHH:MM[:SS]`, with no zone.
pub fn civil_time(at: &str) -> bool {
    let b = at.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let number = |r: std::ops::Range<usize>| at.get(r).and_then(|s| s.parse::<u32>().ok()).unwrap_or(99);
    if days(at).is_none() || !matches!(b.len(), 10 | 16 | 19) || !digits(0..4) {
        return false;
    }
    if b.len() == 10 {
        return true;
    }
    if b[10] != b'T' || b[13] != b':' || !digits(11..13) || !digits(14..16) || number(11..13) > 23 {
        return false;
    }
    if number(14..16) > 59 {
        return false;
    }
    b.len() == 16 || (b[16] == b':' && digits(17..19) && number(17..19) <= 59)
}

/// The city a time zone's IANA name ends in, as a person says it: `America/New_York` is New York.
pub fn city(zone: &str) -> String {
    zone.rsplit('/').next().unwrap_or(zone).replace('_', " ")
}

/// Which of a service's terms a fetch came under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Terms {
    /// The service's free terms: Open-Meteo's, for non-commercial use.
    Free,
    /// The terms of the user's own subscription: Open-Meteo's `customer-` hosts.
    Customer,
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

/// The `cell` of the layer the credit at a chart's or a table's foot is drawn in (ADR-0030): what a
/// tagged PDF reads as a paragraph after the figure or the table, apart from the table's cells.
pub const FOOT_CELL: [u32; 2] = [u32::MAX, u32::MAX];

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
pub const SERVICES: &[Service] = &[
    Service { name: "geoapify", title: "Geoapify", credited: true },
    Service { name: "open-meteo", title: "Open-Meteo", credited: true },
    Service { name: "twelve-data", title: "Twelve Data", credited: true },
];

/// The service `name`, if this build knows it.
pub fn service(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.name == name)
}

/// The credit drawn at the foot of a chart or a table whose props are `props` (ADR-0030, decision
/// 6): none where the data source it reads was fetched from no service. A forecast's names its
/// place and the forecast's date there first; then the service's credit; then "edited" where its
/// rows were edited since they came, as CC BY asks.
pub fn reads(deck: &crate::Deck, props: &crate::document::Props) -> Option<Vec<CreditRun>> {
    let name = props.get("data").and_then(Value::as_str)?.strip_prefix('@')?;
    let fetched = deck.data.get(name)?.fetched()?;
    Some(foot(&fetched, deck.meta.as_ref().and_then(|m| m.lang.as_deref())))
}

/// The runs of the credit a fetched data source's `fetched` record draws at the foot of what reads
/// it, in the deck's language `lang` (ADR-0030).
pub fn foot(fetched: &Fetched, lang: Option<&str>) -> Vec<CreditRun> {
    let mut out = Vec::new();
    let plain = |text: String| CreditRun { text, link: None };
    if let Some(forecast) = &fetched.forecast {
        let place = &forecast.place;
        let named = match &place.region {
            Some(region) if !region.is_empty() && *region != place.name => format!("{}, {region}", place.name),
            _ => place.name.clone(),
        };
        out.push(plain(named));
        if let Some(at) = seconds(&fetched.at) {
            let day = civil((at + i64::from(place.offset)).div_euclid(86_400));
            out.push(plain(format!(" · {}", said_day(&day, lang))));
        }
        out.push(plain(" · ".into()));
    }
    out.extend(fetched.credit.iter().filter(|r| !r.text.is_empty()).cloned());
    // A price series says how far its prices run, in the exchange's own time (ADR-0030, decision
    // 23): "prices to Oct 15, 2026, 4:00 PM New York time".
    if let Some(stock) = &fetched.stock {
        let day = said_day(stock.through.get(..10).unwrap_or(&stock.through), lang);
        let time = stock.through.get(11..16).map(|t| clock(t, lang));
        let to = match time {
            Some(time) => format!("{day}, {time} {} time", city(&stock.zone)),
            None => day,
        };
        out.push(plain(format!(" · prices to {to}")));
    }
    if fetched.edited {
        out.push(plain(" · edited".into()));
    }
    out
}

/// A day, `YYYY-MM-DD`, as a chart's foot prints it in language `lang`: `Oct 15, 2026` in American
/// English, `15 Oct 2026` elsewhere, in the language's own short month.
pub fn said_day(day: &str, lang: Option<&str>) -> String {
    let locale = crate::format::Locale::of(lang);
    let part = |r: std::ops::Range<usize>| day.get(r).and_then(|p| p.parse::<usize>().ok());
    let (Some(y), Some(m @ 1..=12), Some(d)) = (part(0..4), part(5..7), part(8..10)) else { return day.into() };
    let month = locale.short_months[m - 1];
    match locale.tag {
        "en-US" => format!("{month} {d}, {y}"),
        _ => format!("{d} {month} {y}"),
    }
}

/// A time of day, `HH:MM`, as a chart's foot prints it in language `lang`: `4:00 PM` in English,
/// `16:00` elsewhere.
fn clock(hm: &str, lang: Option<&str>) -> String {
    let (Some(h), Some(m)) = (hm.get(0..2).and_then(|h| h.parse::<u32>().ok()), hm.get(3..5)) else {
        return hm.into();
    };
    match crate::format::Locale::of(lang).tag {
        tag if tag.starts_with("en") => {
            let twelve = if h % 12 == 0 { 12 } else { h % 12 };
            format!("{twelve}:{m} {}", if h < 12 { "AM" } else { "PM" })
        }
        _ => hm.into(),
    }
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
    let (month, day, hour, minute, second) =
        (number(5..7), number(8..10), number(11..13), number(14..16), number(17..19));
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
            [h1, h2, m1, m2].iter().all(|d| d.is_ascii_digit())
                && number(i + 1..i + 3) <= 23
                && number(i + 4..i + 6) <= 59
        }
        _ => false,
    }
}

/// Seconds since the epoch at instant `at` (RFC 3339), if it is one.
pub fn seconds(at: &str) -> Option<i64> {
    if !instant(at) {
        return None;
    }
    let number = |r: std::ops::Range<usize>| at.get(r).and_then(|s| s.parse::<i64>().ok());
    let days = days(&at[..10])?;
    let (h, m, s) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let offset = match at.as_bytes()[at.len() - 1] {
        b'Z' | b'z' => 0,
        _ => {
            let o = &at[at.len() - 6..];
            let sign = if o.starts_with('-') { -1 } else { 1 };
            sign * (o[1..3].parse::<i64>().ok()? * 3600 + o[4..6].parse::<i64>().ok()? * 60)
        }
    };
    Some(days * 86_400 + h * 3600 + m * 60 + s - offset)
}

/// A moment lint judges dates at (`scaena lint --now`, `deck_lint`'s `now`): an instant as RFC 3339
/// writes one, or a day, `YYYY-MM-DD`, at its start in UTC; seconds since the epoch.
pub fn moment(now: &str) -> Option<i64> {
    match now.len() {
        10 => days(now).map(|d| d * 86_400),
        _ => seconds(now),
    }
}

/// Days since 1970-01-01 of the civil day `date` (`YYYY-MM-DD`, or the date at the head of a
/// civil time), if it is one.
pub fn days(date: &str) -> Option<i64> {
    let part = |r: std::ops::Range<usize>| date.get(r).and_then(|s| s.parse::<i64>().ok());
    let b = date.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' || !date[..4].bytes().all(|d| d.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (part(0..4)?, part(5..7)?, part(8..10)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The civil day `days` after 1970-01-01, as `YYYY-MM-DD`.
pub fn civil(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Whether a forecast has passed at `now` (seconds since the epoch, ADR-0030 decision 21): the day
/// of its first period, `first` (a date or a civil time), is before the day at its place, `now`
/// plus the offset its days were cut by. A record kept as of its date has not.
pub fn passed(fetched: &Fetched, first: Option<&str>, now: i64) -> bool {
    let Some(forecast) = fetched.forecast.as_ref().filter(|_| !fetched.kept) else { return false };
    let offset = i64::from(forecast.place.offset);
    // Without rows, the day it was fetched at its place.
    let first = match first.and_then(days) {
        Some(day) => day,
        None => match seconds(&fetched.at) {
            Some(at) => (at + offset).div_euclid(86_400),
            None => return false,
        },
    };
    first < (now + offset).div_euclid(86_400)
}

/// Whether a price series is older at `now` (seconds since the epoch) than its range allows
/// (ADR-0030, decision 25): more days from its `through` to the day `now` falls on in UTC than
/// [`Range::allowed`]. A series kept as of its date, or one whose `through` is no civil time, is
/// not.
pub fn stale(fetched: &Fetched, now: i64) -> bool {
    let Some(stock) = fetched.stock.as_ref().filter(|_| !fetched.kept) else { return false };
    let Some(through) = days(&stock.through).filter(|_| civil_time(&stock.through)) else { return false };
    now.div_euclid(86_400) - through > stock.range.allowed()
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
        out.push((
            "/from".into(),
            format!("`{}` is not a service this build knows: {}", fetched.from, names.join(", ")),
        ));
    }
    if !instant(&fetched.at) {
        out.push(("/at".into(), format!("`{}` is not an instant: write it as 2026-10-15T20:12:44Z", fetched.at)));
    }
    if let Some(stock) = fetched.stock.as_ref().filter(|s| !civil_time(&s.through)) {
        let at = &stock.through;
        out.push((
            "/stock/through".into(),
            format!("`{at}` is no civil time: write it as 2026-10-15 or 2026-10-15T16:00"),
        ));
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
        for at in
            ["2026-10-15T20:12:44Z", "2026-10-15T20:12:44.123Z", "2026-10-15T20:12:44+02:00", "2026-10-15t20:12:44z"]
        {
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

    fn prices(range: &str, through: &str, kept: bool) -> Fetched {
        serde_json::from_value(json!({
            "from": "twelve-data", "at": "2026-10-15T20:12:44Z", "asked": "AAPL", "kept": kept,
            "credit": [{ "text": "Data provided by Twelve Data", "link": "https://twelvedata.com" }],
            "stock": { "symbol": "AAPL", "range": range, "exchange": "NASDAQ", "currency": "USD",
                       "zone": "America/New_York", "through": through }
        }))
        .unwrap()
    }

    #[test]
    fn a_price_series_says_how_far_its_prices_run_and_when_they_are_old() {
        let day = prices("1d", "2026-10-15T16:00", false);
        let words = said(&foot(&day, Some("en-US")));
        assert_eq!(words, "Data provided by Twelve Data · prices to Oct 15, 2026, 4:00 PM New York time");
        assert!(said(&foot(&day, Some("de-DE"))).ends_with(", 16:00 New York time"), "a 24-hour clock");
        let year = prices("1y", "2026-10-15", false);
        assert!(said(&foot(&year, Some("en-US"))).ends_with(" · prices to Oct 15, 2026"));
        assert_eq!(foot(&day, None)[0].link.as_deref(), Some("https://twelvedata.com"), "the credit links");
        // A day's and a week's prices may be a day old; a month's, a quarter's, and the year to
        // date's a week; a year's a month (ADR-0030, decision 25).
        let at = |d: &str| seconds(&format!("{d}T12:00:00Z")).unwrap();
        for (range, fresh, old) in [
            ("1d", "2026-10-16", "2026-10-17"),
            ("1w", "2026-10-16", "2026-10-17"),
            ("1m", "2026-10-22", "2026-10-23"),
            ("3m", "2026-10-22", "2026-10-23"),
            ("ytd", "2026-10-22", "2026-10-23"),
            ("1y", "2026-11-15", "2026-11-16"),
        ] {
            let series = prices(range, "2026-10-15T16:00", false);
            assert!(!stale(&series, at(fresh)), "{range} on {fresh}");
            assert!(stale(&series, at(old)), "{range} on {old}");
            assert!(!stale(&prices(range, "2026-10-15", true), at(old)), "kept, {range}");
        }
        assert!(!stale(&prices("1d", "Thursday", false), at("2027-01-01")), "no civil time, no date judged");
        assert!(!passed(&day, None, at("2027-01-01")), "a price series is no forecast");
    }

    #[test]
    fn a_civil_time_is_a_day_or_a_day_and_a_time_with_no_zone() {
        for at in ["2026-10-15", "2026-10-15T16:00", "2026-10-15T09:35:00"] {
            assert!(civil_time(at), "{at}");
        }
        for at in ["2026-10-15T16:00Z", "2026-10-15 16:00", "2026-10-15T24:00", "2026-13-01", "2026-10-15T16", "today"]
        {
            assert!(!civil_time(at), "{at}");
        }
        assert_eq!(city("America/New_York"), "New York");
        assert_eq!(city("Europe/London"), "London");
        let wrong = json!({ "from": "twelve-data", "at": "2026-10-15T20:12:44Z", "asked": "AAPL",
                            "credit": [{ "text": "Data provided by Twelve Data" }],
                            "stock": { "symbol": "AAPL", "range": "1d", "zone": "America/New_York", "through": "4 PM" } });
        assert_eq!(problems(&wrong)[0].0, "/stock/through");
        let range = json!({ "symbol": "AAPL", "range": "2y", "zone": "America/New_York", "through": "2026-10-15" });
        assert!(serde_json::from_value::<Stock>(range).is_err(), "an unknown range");
    }

    #[test]
    fn instants_and_days_count_from_the_epoch() {
        assert_eq!(seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(seconds("2026-10-15T20:12:44Z"), Some(1_792_095_164));
        assert_eq!(seconds("2026-10-15T16:12:44-04:00"), seconds("2026-10-15T20:12:44Z"));
        assert_eq!(seconds("today"), None);
        assert_eq!(moment("2026-10-15"), seconds("2026-10-15T00:00:00Z"));
        assert_eq!(moment("2026-10-15T20:12:44.5Z"), seconds("2026-10-15T20:12:44Z"));
        for day in ["1970-01-01", "2000-02-29", "2026-10-15", "1969-12-31", "2100-03-01"] {
            assert_eq!(civil(days(day).unwrap()), day);
        }
        assert_eq!(days("2026-13-01"), None);
        assert_eq!(said_day("2026-10-15", Some("en-US")), "Oct 15, 2026");
        assert_eq!(said_day("2026-10-15", Some("en-GB")), "15 Oct 2026");
    }

    fn forecast(name: &str, offset: i32, first: &str) -> (Fetched, String) {
        let record = json!({
            "from": "open-meteo", "at": "2026-10-15T12:00:00Z", "asked": name,
            "credit": [{ "text": "Weather data by Open-Meteo.com", "link": "https://open-meteo.com/" }],
            "forecast": { "place": { "name": name, "lat": 0, "lon": 0, "zone": "Etc/UTC", "offset": offset },
                          "period": "day", "units": "metric" }
        });
        (serde_json::from_value(record).unwrap(), first.to_string())
    }

    /// A day is over at a place by its own clock (ADR-0030, decision 21): 05:00 on Oct 16 in UTC is
    /// past midnight in Boston, and still Oct 15 in Honolulu.
    #[test]
    fn a_day_is_over_in_boston_but_not_in_honolulu() {
        let now = seconds("2026-10-16T05:00:00Z").unwrap();
        let (boston, first) = forecast("Boston", -4 * 3600, "2026-10-15");
        let (honolulu, _) = forecast("Honolulu", -10 * 3600, "2026-10-15");
        assert!(passed(&boston, Some(&first), now));
        assert!(!passed(&honolulu, Some(&first), now));
        // Kept as of its date, it has not passed.
        assert!(!passed(&Fetched { kept: true, ..boston.clone() }, Some(&first), now));
        // Without rows, the day it was fetched there.
        assert!(passed(&boston, None, now));
        // A map has no date to pass.
        assert!(!passed(&Fetched { forecast: None, ..boston }, Some(&first), now));
    }

    #[test]
    fn a_forecasts_foot_names_its_place_and_its_day_there() {
        let (mut boston, _) = forecast("Boston", -4 * 3600, "2026-10-15");
        boston.at = "2026-10-16T02:00:00Z".into();
        let said = |f: &Fetched| said(&foot(f, Some("en-US")));
        assert_eq!(said(&boston), "Boston · Oct 15, 2026 · Weather data by Open-Meteo.com");
        boston.edited = true;
        assert_eq!(said(&boston), "Boston · Oct 15, 2026 · Weather data by Open-Meteo.com · edited");
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
