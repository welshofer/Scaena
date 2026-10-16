//! # scaena-objects
//!
//! Objects from outside the deck (ADR-0030), each fetched once by a plan the client runs: Rust
//! builds each request and reads each answer, and the client makes the call, as `scaena-sources`'
//! plans and `scaena-chat` work. Nothing here makes an HTTP call, reads a clock, or holds a key.
//!
//! - A [`Get`] is a request: its URL, its headers, the most bytes its answer may hold, and the slot
//!   the user's key fills ([`Key`]), a query parameter or a header, which the client fills as it
//!   sends. So no URL, error, or log that Rust writes holds a key.
//! - A plan reads each [`Answer`] into its next [`Step`]: another request, the places to choose
//!   from where a name matches several, or what was fetched, typed. A refusal is said in the
//!   service's own words where it gives them.
//! - [`run`] hands each answer back over a [`Transport`] the client gives: the CLI's makes each
//!   request with `ureq`, and the MCP server goes through the CLI's.
//!
//! [`map`] is Geoapify's map: its geocoder, then its static map.

pub mod map;

use serde::Serialize;

/// Where the user's key goes in a request: a query parameter, or a header, by its name. The
/// client puts it there as it sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "in", content = "name", rename_all = "lowercase")]
pub enum Key {
    Query(String),
    Header(String),
}

/// A request the client makes: a GET of `url` with `headers`, its answer held to `most` bytes, the
/// user's key for `service` in `key`'s slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Get {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub most: u64,
    /// The service whose key it takes, by Scaena's name for it (`scaena_core::fetched::SERVICES`).
    pub service: String,
    pub key: Option<Key>,
}

impl Get {
    /// The URL to send, `key` in its query where the key goes there: what a client sends and never
    /// writes down.
    pub fn keyed_url(&self, key: &str) -> String {
        match &self.key {
            Some(Key::Query(name)) => {
                let join = if self.url.contains('?') { '&' } else { '?' };
                format!("{}{join}{name}={}", self.url, encode(key))
            }
            _ => self.url.clone(),
        }
    }

    /// The headers to send, `key` among them where the key goes in one.
    pub fn keyed_headers(&self, key: &str) -> Vec<(String, String)> {
        let mut headers = self.headers.clone();
        if let Some(Key::Header(name)) = &self.key {
            headers.push((name.clone(), key.to_string()));
        }
        headers
    }
}

/// A service's answer: its status, its content type, and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub kind: String,
    pub body: Vec<u8>,
}

/// What makes each request: the client's, with the user's key for each service it has one for.
pub trait Transport {
    /// Whether it holds the user's key for `service`: without one, nothing is asked of it.
    fn keyed(&self, service: &str) -> bool;
    /// `get` sent, the key in its slot: the answer, or why none came, without the key.
    fn send(&self, get: &Get) -> Result<Answer, String>;
    /// The instant now, by the client's clock, as RFC 3339 writes it in UTC: when an answer came,
    /// which a record keeps. The plans read no clock.
    fn now(&self) -> String;
}

/// A place a name matches, among several to choose from.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Found {
    /// Its name, as the geocoder gives it.
    pub name: String,
    /// Where it is, in words: its address, as the geocoder writes it.
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    /// What it is: a city, a street, an amenity, as the geocoder says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// What a plan asks for next.
#[derive(Debug, Clone, PartialEq)]
pub enum Step<T> {
    /// Another request.
    Get(Get),
    /// The places a name matched, to choose one of and ask again.
    Choose(Vec<Found>),
    /// What was fetched.
    Done(T),
}

/// Why a plan stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectError {
    /// What was asked is not something the plan can ask for.
    #[error("{0}")]
    Asked(String),
    /// The service refused it, in its own words where it gave them.
    #[error("{service} refused it: {words}")]
    Refused { service: String, status: u16, words: String },
    /// It answered with what the plan cannot read.
    #[error("{0}")]
    Unreadable(String),
    /// No answer came: the network, or the client.
    #[error("{0}")]
    Unreached(String),
    /// No key for the service is kept.
    #[error("no {service} key: {how}")]
    NoKey { service: String, how: String },
}

/// A plan the client runs.
pub trait Plan {
    type Done;
    /// The answer to the last request: what to do next.
    fn answer(&mut self, answer: &Answer) -> Result<Step<Self::Done>, ObjectError>;
}

/// What running a plan came to: the places to choose from, or what was fetched.
#[derive(Debug, Clone, PartialEq)]
pub enum Ran<T> {
    Choose(Vec<Found>),
    Done(T),
}

/// `plan` run from `first` over `transport`: each request sent, each answer handed back.
pub fn run<P: Plan>(plan: &mut P, first: Get, transport: &dyn Transport) -> Result<Ran<P::Done>, ObjectError> {
    let mut get = first;
    loop {
        let answer = transport.send(&get).map_err(ObjectError::Unreached)?;
        if answer.body.len() as u64 > get.most {
            return Err(ObjectError::Unreadable(format!("{} answered with more than {} bytes", get.url, get.most)));
        }
        match plan.answer(&answer)? {
            Step::Get(next) => get = next,
            Step::Choose(found) => return Ok(Ran::Choose(found)),
            Step::Done(done) => return Ok(Ran::Done(done)),
        }
    }
}

/// `s` percent-encoded for a URL's query: everything but the unreserved characters.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(char::from(b)),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// What the CLI calls itself in its requests: Scaena and its version. A browser's plan sets no
/// User-Agent: one a page sets is a header a service must give leave for first (CORS).
pub fn user_agent() -> String {
    format!("Scaena/{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_goes_where_its_slot_says_and_nowhere_rust_writes() {
        let get = Get {
            url: "https://maps.example/v1/map?x=1".into(),
            headers: vec![],
            most: 10,
            service: "geoapify".into(),
            key: Some(Key::Query("apiKey".into())),
        };
        assert_eq!(get.keyed_url("a b&c"), "https://maps.example/v1/map?x=1&apiKey=a%20b%26c");
        assert!(!get.url.contains("apiKey"));
        let header = Get { key: Some(Key::Header("Authorization".into())), ..get };
        assert_eq!(header.keyed_url("k"), header.url);
        assert_eq!(header.keyed_headers("k"), [("Authorization".to_string(), "k".to_string())]);
        assert_eq!(encode("Café 1/2"), "Caf%C3%A9%201%2F2");
    }
}
