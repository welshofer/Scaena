//! `scaena map` (ADR-0030, PLAN O.1b): a map from Geoapify put on a slide, each request the plan
//! builds (`scaena_objects::map`) made here over `ureq`, the key from `$GEOAPIFY_API_KEY` put in
//! its slot as it is sent and written nowhere. `scaena mcp` hands the same transport to the server.

use scaena_objects::{Answer, Get, Transport};

/// The CLI's transport: `ureq`, with the system's certificates and the environment's proxy, the
/// user's keys from the environment. `$SCAENA_GEOAPIFY_BASE` sends Geoapify's requests to another
/// address, where a test serves recorded answers.
pub struct Ureq {
    agent: ureq::Agent,
    geoapify: Option<String>,
    base: Option<String>,
}

impl Ureq {
    pub fn new() -> Ureq {
        let key = std::env::var("GEOAPIFY_API_KEY").ok().filter(|k| !k.trim().is_empty());
        let base = std::env::var("SCAENA_GEOAPIFY_BASE").ok().filter(|b| !b.is_empty());
        Ureq { agent: crate::generate::agent(), geoapify: key.map(|k| k.trim().to_string()), base }
    }

    /// Where `url` goes: to the stand-in, where one is named.
    fn routed(&self, url: String) -> String {
        match &self.base {
            Some(base) => {
                let base = base.trim_end_matches('/');
                ["https://api.geoapify.com", "https://maps.geoapify.com"]
                    .iter()
                    .find_map(|host| url.strip_prefix(host).map(|rest| format!("{base}{rest}")))
                    .unwrap_or(url)
            }
            None => url,
        }
    }
}

impl Transport for Ureq {
    fn keyed(&self, service: &str) -> bool {
        service == "geoapify" && self.geoapify.is_some()
    }

    fn send(&self, get: &Get) -> Result<Answer, String> {
        let key = match get.service.as_str() {
            "geoapify" => self.geoapify.as_deref(),
            _ => None,
        };
        let (url, headers) = match key {
            Some(key) => (get.keyed_url(key), get.keyed_headers(key)),
            None => (get.url.clone(), get.headers.clone()),
        };
        let mut request = self.agent.get(self.routed(url));
        for (k, v) in &headers {
            request = request.header(k, v);
        }
        // An error says the URL as Rust wrote it, and never the key.
        let hidden = |e: ureq::Error| {
            let said = format!("{}: {e}", get.url);
            match key {
                Some(key) => said.replace(key, "…"),
                None => said,
            }
        };
        let mut response = request.call().map_err(hidden)?;
        let status = response.status().as_u16();
        let kind =
            (response.headers().get("content-type").and_then(|v| v.to_str().ok())).unwrap_or_default().to_string();
        let body = (response.body_mut().with_config().limit(get.most).read_to_vec()).map_err(hidden)?;
        Ok(Answer { status, kind, body })
    }

    fn now(&self) -> String {
        crate::now_rfc3339()
    }
}
