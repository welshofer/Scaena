//! A forecast fetched into the deck (ADR-0030, decisions 18–21; PLAN O.2a): what its data source's
//! `fetched.forecast` records, and the WMO weather codes its `sky` column holds, each to a sky, its
//! icon by day and by night, and its words in each language `docs/spec/format.md` lists.
//!
//! A forecast's rows are written inline in its data source. By the day: `date`, `sky` (the WMO
//! code), `high`, `low`, `rain` (the chance, in percent), `amount`, `wind`, and `direction`. By the
//! hour: `time`, `sky`, `temperature`, `rain`, `amount`, `wind`, `direction`, and `day` (daylight).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a forecast was fetched for (ADR-0030, decision 18): the place, the period, and the units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Forecast {
    pub place: Place,
    /// Whether each row is a day or an hour.
    pub period: Period,
    /// The units its figures came in.
    pub units: Units,
}

/// The place a forecast is for, as the geocoder named it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Place {
    /// Its name: `Boston`.
    pub name: String,
    /// The region it is in, where the geocoder names one: `Massachusetts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Its country: `United States`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[schemars(range(min = -90, max = 90))]
    pub lat: f64,
    #[schemars(range(min = -180, max = 180))]
    pub lon: f64,
    /// Its time zone, by its IANA name: `America/New_York`.
    pub zone: String,
    /// The offset from UTC, in seconds, its days were cut by when it was fetched: `-14400`.
    #[schemars(range(min = -64800, max = 64800))]
    pub offset: i32,
}

/// A forecast's period: each row a day or an hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Day,
    Hour,
}

/// The units a forecast's figures are in: metric (°C, km/h, mm) or US (°F, mph, inches).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    Metric,
    Us,
}

/// What a WMO code says the sky does, as a forecast draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Overcast,
    Fog,
    Rain,
    Snow,
    Thunder,
}

impl Sky {
    /// Its icon, by Phosphor's name: by day, or by night where the night has one of its own.
    pub fn icon(self, day: bool) -> &'static str {
        match (self, day) {
            (Sky::Clear, true) => "sun",
            (Sky::Clear, false) => "moon",
            (Sky::PartlyCloudy, true) => "cloud-sun",
            (Sky::PartlyCloudy, false) => "cloud-moon",
            (Sky::Overcast, _) => "cloud",
            (Sky::Fog, _) => "cloud-fog",
            (Sky::Rain, _) => "cloud-rain",
            (Sky::Snow, _) => "cloud-snow",
            (Sky::Thunder, _) => "cloud-lightning",
        }
    }
}

/// A WMO weather code Open-Meteo documents: the code, its sky, and its words in English, German,
/// French, Spanish, and Dutch, the languages of `docs/spec/format.md` (`en-US` and `en-GB` say
/// the same).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code {
    pub code: u8,
    pub sky: Sky,
    /// `[en, de, fr, es, nl]`.
    pub words: [&'static str; 5],
}

/// Every WMO code Open-Meteo documents (https://open-meteo.com/en/docs, "WMO Weather
/// interpretation codes").
pub const CODES: &[Code] = &[
    Code { code: 0, sky: Sky::Clear, words: ["Clear", "Klar", "Dégagé", "Despejado", "Helder"] },
    Code {
        code: 1,
        sky: Sky::Clear,
        words: ["Mostly clear", "Überwiegend klar", "Plutôt dégagé", "Mayormente despejado", "Overwegend helder"],
    },
    Code {
        code: 2,
        sky: Sky::PartlyCloudy,
        words: ["Partly cloudy", "Teilweise bewölkt", "Partiellement nuageux", "Parcialmente nublado", "Half bewolkt"],
    },
    Code { code: 3, sky: Sky::Overcast, words: ["Overcast", "Bedeckt", "Couvert", "Cubierto", "Bewolkt"] },
    Code { code: 45, sky: Sky::Fog, words: ["Fog", "Nebel", "Brouillard", "Niebla", "Mist"] },
    Code {
        code: 48,
        sky: Sky::Fog,
        words: ["Freezing fog", "Gefrierender Nebel", "Brouillard givrant", "Niebla helada", "Aanvriezende mist"],
    },
    Code {
        code: 51,
        sky: Sky::Rain,
        words: ["Light drizzle", "Leichter Nieselregen", "Bruine légère", "Llovizna ligera", "Lichte motregen"],
    },
    Code { code: 53, sky: Sky::Rain, words: ["Drizzle", "Nieselregen", "Bruine", "Llovizna", "Motregen"] },
    Code {
        code: 55,
        sky: Sky::Rain,
        words: ["Heavy drizzle", "Starker Nieselregen", "Forte bruine", "Llovizna intensa", "Zware motregen"],
    },
    Code {
        code: 56,
        sky: Sky::Rain,
        words: [
            "Light freezing drizzle",
            "Leichter gefrierender Nieselregen",
            "Bruine verglaçante légère",
            "Llovizna helada ligera",
            "Lichte ijzel",
        ],
    },
    Code {
        code: 57,
        sky: Sky::Rain,
        words: ["Freezing drizzle", "Gefrierender Nieselregen", "Bruine verglaçante", "Llovizna helada", "IJzel"],
    },
    Code {
        code: 61,
        sky: Sky::Rain,
        words: ["Light rain", "Leichter Regen", "Pluie légère", "Lluvia ligera", "Lichte regen"],
    },
    Code { code: 63, sky: Sky::Rain, words: ["Rain", "Regen", "Pluie", "Lluvia", "Regen"] },
    Code {
        code: 65,
        sky: Sky::Rain,
        words: ["Heavy rain", "Starker Regen", "Forte pluie", "Lluvia intensa", "Zware regen"],
    },
    Code {
        code: 66,
        sky: Sky::Rain,
        words: [
            "Light freezing rain",
            "Leichter Eisregen",
            "Pluie verglaçante légère",
            "Lluvia helada ligera",
            "Lichte aanvriezende regen",
        ],
    },
    Code {
        code: 67,
        sky: Sky::Rain,
        words: ["Freezing rain", "Eisregen", "Pluie verglaçante", "Lluvia helada", "Aanvriezende regen"],
    },
    Code {
        code: 71,
        sky: Sky::Snow,
        words: ["Light snow", "Leichter Schneefall", "Neige légère", "Nieve ligera", "Lichte sneeuw"],
    },
    Code { code: 73, sky: Sky::Snow, words: ["Snow", "Schneefall", "Neige", "Nieve", "Sneeuw"] },
    Code {
        code: 75,
        sky: Sky::Snow,
        words: ["Heavy snow", "Starker Schneefall", "Forte neige", "Nevada intensa", "Zware sneeuw"],
    },
    Code {
        code: 77,
        sky: Sky::Snow,
        words: ["Snow grains", "Schneegriesel", "Neige en grains", "Cinarra", "Motsneeuw"],
    },
    Code {
        code: 80,
        sky: Sky::Rain,
        words: ["Light showers", "Leichte Schauer", "Averses légères", "Chubascos ligeros", "Lichte buien"],
    },
    Code { code: 81, sky: Sky::Rain, words: ["Showers", "Schauer", "Averses", "Chubascos", "Buien"] },
    Code {
        code: 82,
        sky: Sky::Rain,
        words: ["Heavy showers", "Starke Schauer", "Fortes averses", "Chubascos fuertes", "Zware buien"],
    },
    Code {
        code: 85,
        sky: Sky::Snow,
        words: ["Snow showers", "Schneeschauer", "Averses de neige", "Chubascos de nieve", "Sneeuwbuien"],
    },
    Code {
        code: 86,
        sky: Sky::Snow,
        words: [
            "Heavy snow showers",
            "Starke Schneeschauer",
            "Fortes averses de neige",
            "Chubascos de nieve fuertes",
            "Zware sneeuwbuien",
        ],
    },
    Code { code: 95, sky: Sky::Thunder, words: ["Thunderstorm", "Gewitter", "Orage", "Tormenta", "Onweer"] },
    Code {
        code: 96,
        sky: Sky::Thunder,
        words: [
            "Thunderstorm with hail",
            "Gewitter mit Hagel",
            "Orage avec grêle",
            "Tormenta con granizo",
            "Onweer met hagel",
        ],
    },
    Code {
        code: 99,
        sky: Sky::Thunder,
        words: [
            "Thunderstorm with heavy hail",
            "Gewitter mit starkem Hagel",
            "Orage avec forte grêle",
            "Tormenta con granizo fuerte",
            "Onweer met zware hagel",
        ],
    },
];

/// The WMO code `code`, if Open-Meteo documents it.
pub fn code(code: f64) -> Option<&'static Code> {
    CODES.iter().find(|c| f64::from(c.code) == code)
}

impl Code {
    /// Its words in the deck's language (`meta.lang`), read as `docs/spec/format.md` reads one:
    /// its language's entry, else English.
    pub fn said(&self, lang: Option<&str>) -> &'static str {
        let lang = lang.unwrap_or("en").split(['-', '_']).next().unwrap_or("en").to_ascii_lowercase();
        let i = match lang.as_str() {
            "de" => 1,
            "fr" => 2,
            "es" => 3,
            "nl" => 4,
            _ => 0,
        };
        self.words[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_open_meteo_documents_has_a_sky_and_words() {
        let documented = [
            0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82, 85, 86, 95, 96, 99,
        ];
        let ours: Vec<u8> = CODES.iter().map(|c| c.code).collect();
        assert_eq!(ours, documented);
        for c in CODES {
            assert!(c.words.iter().all(|w| !w.is_empty()), "{}", c.code);
        }
        assert_eq!(code(61.0).map(|c| c.sky), Some(Sky::Rain));
        assert!(code(4.0).is_none());
        assert_eq!(code(0.0).unwrap().said(Some("de-DE")), "Klar");
        assert_eq!(code(0.0).unwrap().said(Some("en-GB")), "Clear");
        assert_eq!(code(95.0).unwrap().said(Some("pt-BR")), "Thunderstorm");
        assert_eq!(Sky::Clear.icon(false), "moon");
        assert_eq!(Sky::Rain.icon(false), "cloud-rain");
    }
}
