//! The sources `scaena generate` grounds a deck in (PLAN G.3, G.4, ADR-0026), read or fetched
//! here: a file, read by `scaena-sources`; and, by the plan `scaena_sources::fetch` makes of
//! each, a company's filings on SEC's EDGAR, a GitHub repository, a Google file shared by link,
//! and a web page, each request made by the CLI. Packing them and keeping their sheets as data
//! sources are `scaena_ops::narrative`'s.

use anyhow::{Context, Result, anyhow, bail};
use scaena_sources::Source;
use scaena_sources::fetch::{Fetch, Get, Reach, Remote, Step};
use std::path::Path;

/// The source `spec` names: `sec:TICKER` (or `sec:TICKER:10-K,8-K`), `github:OWNER/REPO` or a
/// GitHub address, a Google file shared by link, a web address, or a file; each request made by
/// `agent`.
pub fn read(spec: &str, agent: &ureq::Agent, reach: &Reach) -> Result<Source> {
    match scaena_sources::fetch::remote(spec).map_err(|e| anyhow!("{e}"))? {
        Some(remote) => {
            if matches!(remote, Remote::Sec { .. }) && reach.contact.is_none() {
                bail!(
                    "SEC asks each request to say who makes it: give --sec-contact (or $SCAENA_SEC_CONTACT) your email"
                );
            }
            let (mut fetch, mut get) = Fetch::new(remote, reach).map_err(|e| anyhow!("{e}"))?;
            loop {
                let (status, kind, body) = fetched(agent, &get)?;
                match fetch.answer(status, &kind, &body).map_err(|e| anyhow!("{e}"))? {
                    Step::Get(next) => get = next,
                    Step::Done(source) => return Ok(source),
                }
            }
        }
        None => {
            let path = Path::new(spec);
            let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            let name = path.file_name().map_or_else(|| spec.to_string(), |n| n.to_string_lossy().into_owned());
            scaena_sources::read(&name, &bytes).map_err(|e| anyhow!("{}: {e}", path.display()))
        }
    }
}

/// `get` made: its status, its content type, and its body.
fn fetched(agent: &ureq::Agent, get: &Get) -> Result<(u16, String, Vec<u8>)> {
    let mut request = agent.get(&get.url);
    for (k, v) in &get.headers {
        request = request.header(k, v);
    }
    let mut response = request.call().map_err(|e| anyhow!("{}: {e}", get.url))?;
    let status = response.status().as_u16();
    let kind = (response.headers().get("content-type").and_then(|v| v.to_str().ok())).unwrap_or_default().to_string();
    let body =
        response.body_mut().with_config().limit(get.most).read_to_vec().map_err(|e| anyhow!("{}: {e}", get.url))?;
    Ok((status, kind, body))
}
