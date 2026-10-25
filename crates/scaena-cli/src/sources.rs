//! The sources `scaena generate` grounds a deck in (PLAN G.3, G.4, ADR-0026), read or fetched
//! here: a file, read by `scaena-sources`; and, by the plan `scaena_sources::fetch` makes of
//! each, a company's filings on SEC's EDGAR, a GitHub repository, a Google file shared by link,
//! and a web page, each request made by the CLI. Packing them and keeping their sheets as data
//! sources are `scaena_ops::narrative`'s.

use anyhow::{Context, Result, anyhow, bail};
use scaena_sources::Source;
use scaena_sources::fetch::{Fetch, Get, Reach, Remote, Step};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::Path;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

/// The client a web source is fetched through: the CLI's ([`crate::generate::agent`]), whose
/// connections go only to an address the public internet reaches (lift-up 10-24b SEC-3). A
/// public name may resolve to this machine or its network (`127.0.0.1.nip.io`, a DNS answer
/// rebound between a check and the GET), so the addresses it connects to are the ones it held
/// to that, not the name.
pub fn agent() -> ureq::Agent {
    ureq::Agent::with_parts(crate::generate::config(), DefaultConnector::default(), Public)
}

/// A resolver that gives back a host's addresses only where the public internet reaches every
/// one, else refuses. A proxy's own host is the user's to name, and resolved as any: the proxy
/// resolves the source's host, which [`resolves_public`] holds to the same before each GET.
#[derive(Debug)]
struct Public;

impl Resolver for Public {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let found = DefaultResolver::default().resolve(uri, config, timeout)?;
        let proxy = config.proxy().is_some_and(|p| uri.host().is_some_and(|h| h.eq_ignore_ascii_case(p.host())));
        if !proxy && let Some(at) = found.iter().find(|a| !scaena_sources::fetch::reached(a.ip())) {
            let host = uri.host().unwrap_or_default();
            return Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                refused(host, at.ip()),
            )));
        }
        Ok(found)
    }
}

/// Why a web source's host is refused: it resolves to `ip`, which the public internet does not
/// reach.
fn refused(host: &str, ip: IpAddr) -> String {
    format!("{host} resolves to {ip}, on this machine or its network: a web source is a public address")
}

/// How a host and port are looked up: the system's resolver ([`system`]), or a test's.
type Lookup = fn(&str, u16) -> std::io::Result<Vec<std::net::SocketAddr>>;

/// A host and port looked up as the system looks them up.
fn system(host: &str, port: u16) -> std::io::Result<Vec<std::net::SocketAddr>> {
    Ok((host, port).to_socket_addrs()?.collect())
}

/// Whether `url`'s host resolves only to addresses the public internet reaches: asked before
/// each GET and each redirect, so a source fetched through a proxy, which resolves the host
/// itself, is held to it too.
fn resolves_public(url: &str, lookup: Lookup) -> Result<()> {
    let uri: ureq::http::Uri = url.parse().map_err(|e| anyhow!("{url}: {e}"))?;
    let host = uri.host().ok_or_else(|| anyhow!("{url}: no host"))?;
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("http") { 80 } else { 443 });
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let found = lookup(bare, port).map_err(|e| anyhow!("{url}: {e}"))?;
    for at in found {
        if !scaena_sources::fetch::reached(at.ip()) {
            bail!("{url}: {}", refused(host, at.ip()));
        }
    }
    Ok(())
}

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

/// The most redirects a GET follows.
const MOST_REDIRECTS: usize = 10;

/// `get` made: its status, its content type, and its body. Each redirect is followed here, held
/// to what a web source may name (`scaena_sources::fetch::redirected`), so a public page cannot
/// send the CLI on to one on this machine or its network.
fn fetched(agent: &ureq::Agent, get: &Get) -> Result<(u16, String, Vec<u8>)> {
    fetched_by(agent, get, system)
}

/// [`fetched`], each host looked up by `lookup`.
fn fetched_by(agent: &ureq::Agent, get: &Get, lookup: Lookup) -> Result<(u16, String, Vec<u8>)> {
    let mut url = get.url.clone();
    let mut headers = get.headers.clone();
    let mut hops = 0;
    let mut response = loop {
        resolves_public(&url, lookup)?;
        let mut request = agent.get(&url).config().max_redirects(0).build();
        for (k, v) in &headers {
            request = request.header(k, v);
        }
        let response = request.call().map_err(|e| anyhow!("{url}: {e}"))?;
        let location = response.headers().get("location").and_then(|v| v.to_str().ok());
        match location {
            Some(to) if response.status().is_redirection() => {
                hops += 1;
                if hops > MOST_REDIRECTS {
                    bail!("{}: redirected more than {MOST_REDIRECTS} times", get.url);
                }
                let next = scaena_sources::fetch::redirected(&url, to).map_err(|e| anyhow!("{e}"))?;
                headers = carried(&url, &next, headers);
                url = next;
            }
            _ => break response,
        }
    };
    let status = response.status().as_u16();
    let kind = (response.headers().get("content-type").and_then(|v| v.to_str().ok())).unwrap_or_default().to_string();
    let body = response.body_mut().with_config().limit(get.most).read_to_vec().map_err(|e| anyhow!("{url}: {e}"))?;
    Ok((status, kind, body))
}

/// The headers a redirect from `from` to `to` carries on: all of them to the same scheme, host,
/// and port, else none that says who the user is (a GitHub token, a proxy's, a cookie), as a
/// browser and ureq's own redirects keep them; once left behind, they stay behind.
fn carried(from: &str, to: &str, headers: Vec<(String, String)>) -> Vec<(String, String)> {
    if origin(from) == origin(to) {
        return headers;
    }
    let secret = |k: &str| ["authorization", "proxy-authorization", "cookie"].iter().any(|s| k.eq_ignore_ascii_case(s));
    headers.into_iter().filter(|(k, _)| !secret(k)).collect()
}

/// A URL's scheme, host, and port, the port its scheme's own where it names none.
fn origin(url: &str) -> Option<(String, String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a).to_ascii_lowercase();
    let (host, port) = match authority.rsplit_once(':').filter(|(_, p)| p.bytes().all(|b| b.is_ascii_digit())) {
        Some((host, port)) if !port.is_empty() => (host.to_string(), port.parse().ok()?),
        _ => (authority.clone(), if scheme == "https" { 443 } else { 80 }),
    };
    Some((scheme, host.trim_end_matches('.').to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A public name that resolves to this machine or its network, as a rebinding DNS answers:
    /// refused before the GET, and when a public page redirects to it, before the redirect is
    /// followed. The page here is this machine's, its name looked up as a public one.
    #[test]
    fn a_name_or_a_redirect_that_resolves_to_this_network_is_refused() {
        fn rebound(host: &str, port: u16) -> std::io::Result<Vec<std::net::SocketAddr>> {
            let ip = match host {
                "127.0.0.1" => "1.1.1.1", // the test's page, looked up as a public one
                "rebind.example.com" => "10.0.0.1",
                "loop.example.com" => "127.0.0.1",
                _ => "93.184.215.14",
            };
            Ok(vec![std::net::SocketAddr::new(ip.parse().unwrap(), port)])
        }
        for name in ["rebind.example.com", "loop.example.com"] {
            let said = resolves_public(&format!("https://{name}/"), rebound).unwrap_err().to_string();
            assert!(said.contains(&format!("{name} resolves to")), "{said}");
        }
        resolves_public("https://www.example.com/", rebound).unwrap();
        // A page that redirects there: the redirect is not followed.
        let page = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = page.local_addr().unwrap().port();
        let served = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = page.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let moved =
                "HTTP/1.1 302 Found\r\nLocation: https://rebind.example.com/internal\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(moved.as_bytes()).unwrap();
        });
        let plain = ureq::Agent::config_builder().proxy(None).http_status_as_error(false).build().new_agent();
        let get = Get { url: format!("http://127.0.0.1:{port}/page"), headers: Vec::new(), most: 1 << 20 };
        let said = fetched_by(&plain, &get, rebound).unwrap_err().to_string();
        assert!(
            said.contains("https://rebind.example.com/internal: rebind.example.com resolves to 10.0.0.1"),
            "{said}"
        );
        served.join().unwrap();
    }

    /// A host that resolves to this machine is refused, however its name passes: by the check
    /// before the GET, and by the agent's own resolver, which never connects to it.
    #[test]
    fn a_source_that_resolves_to_this_machine_is_refused_before_it_is_reached() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        // The CLI's agent, with no proxy from the environment, which would resolve the host itself.
        let agent = || {
            ureq::Agent::with_parts(
                ureq::Agent::config_builder().proxy(None).build(),
                DefaultConnector::default(),
                Public,
            )
        };
        for url in [format!("http://localhost:{port}/"), format!("http://127.0.0.1:{port}/")] {
            let said = resolves_public(&url, system).unwrap_err().to_string();
            // `localhost` may resolve to ::1 first, as it does on CI's Linux.
            assert!(said.contains(" resolves to ") && said.contains("on this machine or its network"), "{said}");
            let get = Get { url: url.clone(), headers: Vec::new(), most: 1 << 20 };
            let said = fetched(&agent(), &get).unwrap_err().to_string();
            assert!(said.contains("on this machine or its network"), "{said}");
            // The agent alone, with no check before it: its resolver refuses, and connects to nothing.
            let said = agent().get(&url).call().unwrap_err().to_string();
            assert!(said.contains("on this machine or its network"), "{url}: {said}");
            assert!(listener.accept().is_err(), "{url}: nothing connected");
        }
        for ip in ["10.0.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "::1", "fe80::1", "::ffff:127.0.0.1"] {
            let host = if ip.contains(':') { format!("[{ip}]") } else { ip.to_string() };
            assert!(resolves_public(&format!("http://{host}/"), system).is_err(), "{ip}");
        }
        // A public address passes, written as one, so no network is asked.
        resolves_public("https://1.1.1.1/", system).unwrap();
        resolves_public("https://[2606:4700:4700::1111]/", system).unwrap();
    }

    #[test]
    fn a_redirect_to_another_origin_leaves_the_users_credentials_behind() {
        let headers = || {
            vec![
                ("user-agent".to_string(), "Scaena".to_string()),
                ("Authorization".to_string(), "Bearer ghp_secret".to_string()),
                ("cookie".to_string(), "session=1".to_string()),
                ("proxy-authorization".to_string(), "Basic x".to_string()),
            ]
        };
        let names = |h: Vec<(String, String)>| h.into_iter().map(|(k, _)| k.to_ascii_lowercase()).collect::<Vec<_>>();
        let api = "https://api.github.com/repos/o/r/readme";
        // The same origin, however it is written: all of them.
        for same in
            ["https://api.github.com/repos/o/r2/readme", "https://API.github.com:443/x", "https://api.github.com./x"]
        {
            assert_eq!(carried(api, same, headers()).len(), 4, "{same}");
        }
        // Another host, scheme, or port: none that says who the user is.
        for other in [
            "https://codeload.github.com/o/r/zip",
            "https://evil.example/steal",
            "http://api.github.com/x",
            "https://api.github.com:8443/x",
            "https://api.github.com@evil.example/x",
        ] {
            assert_eq!(names(carried(api, other, headers())), ["user-agent"], "{other}");
        }
    }
}
