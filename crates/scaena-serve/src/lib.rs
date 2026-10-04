//! `scaena serve` (PLAN 2.11, SPEC §7.1, ADR-0012): the web player and the editor on a bundle's
//! folder, on this machine only.
//!
//! - **The pages** are what `just web` builds (PLAN 2.1–2.9), carried in the binary
//!   ([`pages_built`]): `/` is the player and `/edit` the editor, each on the bundle.
//! - **The bundle** is the folder's files at `/bundle/`, as they are on disk. A page's save
//!   writes them (`PUT`, `DELETE`), inside the folder only.
//! - **The folder is watched.** A `deck.scn` saved in any text editor compiles into `deck.json`,
//!   as `scaena compile` does, and every page hears of each change at `/scaena/events`, as
//!   server-sent events, whoever made it.
//!
//! It listens on 127.0.0.1 alone, and answers only requests that name that host or
//! `localhost`, so no other site's page can reach it through a name of its own. It writes only
//! for a page of its own: a write from any other origin is refused.

mod pages;
mod watch;

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Frame, Incoming};
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};

pub use pages::built as pages_built;
pub use watch::{DECK, Failure, Problem, SOURCE};

/// The largest file a page may write: a picture or a font, with room to spare.
const MAX_FILE: usize = 256 << 20;
/// How often a quiet event stream says it is still there.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// What went wrong before the server could serve.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("{0}: not a bundle's folder (a directory with deck.json in it); `scaena save DECK --to DIR` makes one")]
    NotABundle(PathBuf),
    #[error("127.0.0.1:{port}: {source}; another port: --port")]
    Bind { port: u16, source: std::io::Error },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// What the server tells whoever runs it, as it happens.
#[derive(Clone, Debug)]
pub enum Note {
    /// `deck.scn` compiled, in `ms`; `written` when the deck it says differs from the one in
    /// `deck.json`, which it then replaced.
    Compiled { ms: u64, written: bool },
    /// `deck.scn` does not compile. `deck.json` is as it was.
    Failed(Failure),
    /// Files changed: on disk (`by` is `None`), or by a page's save, `by` its id.
    Changed { paths: Vec<String>, by: Option<String> },
}

/// A bound server, not yet serving.
pub struct Serve {
    listener: TcpListener,
    shared: Arc<Shared>,
}

/// What every request and the folder's watcher share.
pub(crate) struct Shared {
    /// The bundle's folder, its path resolved.
    root: PathBuf,
    port: u16,
    state: Mutex<watch::State>,
    /// Each event for the pages, as server-sent events say it.
    events: broadcast::Sender<Bytes>,
    notes: broadcast::Sender<Note>,
}

type Body = BoxBody<Bytes, Infallible>;

impl Serve {
    /// Bind 127.0.0.1:`port` (0 for any free port) for the bundle in the folder `bundle`.
    pub async fn bind(bundle: &Path, port: u16) -> Result<Serve, ServeError> {
        let not_a_bundle = || ServeError::NotABundle(bundle.to_path_buf());
        let root = bundle.canonicalize().map_err(|_| not_a_bundle())?;
        if !root.join(DECK).is_file() {
            return Err(not_a_bundle());
        }
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await.map_err(|source| ServeError::Bind { port, source })?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            root,
            port,
            state: Mutex::default(),
            events: broadcast::channel(64).0,
            notes: broadcast::channel(64).0,
        });
        Ok(Serve { listener, shared })
    }

    /// Where it answers.
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.shared.port))
    }

    /// What it says as it happens, from now on.
    pub fn notes(&self) -> broadcast::Receiver<Note> {
        self.shared.notes.subscribe()
    }

    /// Serve until the task is dropped: first `deck.scn` compiled if it is newer than
    /// `deck.json`, then the folder watched and each request answered.
    pub async fn run(self) -> Result<(), ServeError> {
        self.shared.start();
        let watched = Arc::downgrade(&self.shared);
        std::thread::spawn(move || watch::run(watched));
        loop {
            let stream = match self.listener.accept().await {
                Ok((stream, _)) => stream,
                // Out of files to open, or a connection gone before it was taken: the next may do.
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let shared = self.shared.clone();
            tokio::spawn(async move {
                let service = service_fn(move |request| {
                    let shared = shared.clone();
                    async move { Ok::<_, Infallible>(shared.answer(request).await) }
                });
                // A page that goes away mid-answer ends its connection; nothing to say.
                let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
            });
        }
    }
}

/// `scaena serve`: bind, tell `started` where, then serve until the process ends, telling
/// `note` what happens.
pub fn run(
    bundle: &Path,
    port: u16,
    started: impl FnOnce(SocketAddr),
    note: impl Fn(Note) + Send + 'static,
) -> Result<(), ServeError> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async {
        let serve = Serve::bind(bundle, port).await?;
        let mut notes = serve.notes();
        started(serve.addr());
        tokio::spawn(async move {
            loop {
                match notes.recv().await {
                    Ok(n) => note(n),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        serve.run().await
    })
}

impl Shared {
    async fn answer(self: Arc<Self>, request: Request<Incoming>) -> Response<Body> {
        if !self.ours(request.headers(), header::HOST, "") {
            return said(StatusCode::FORBIDDEN, "scaena serve answers only to localhost and 127.0.0.1");
        }
        let method = request.method().clone();
        let path = request.uri().path().to_string();
        if let Some(rel) = path.strip_prefix("/bundle/") {
            let Some(rel) = bundle_path(rel) else {
                return said(StatusCode::BAD_REQUEST, "not a path inside the bundle");
            };
            return match method {
                Method::GET | Method::HEAD => self.read(&rel, method == Method::HEAD).await,
                Method::PUT | Method::DELETE => {
                    if !self.ours(request.headers(), header::ORIGIN, "http://") {
                        return said(StatusCode::FORBIDDEN, "scaena serve writes only for its own pages");
                    }
                    let by = request.headers().get("x-scaena-client").and_then(|v| v.to_str().ok()).map(str::to_string);
                    if method == Method::DELETE {
                        let shared = self.clone();
                        return match tokio::task::spawn_blocking(move || shared.remove(&rel, by)).await {
                            Ok(Ok(false)) => said(StatusCode::NOT_FOUND, "no such file in the bundle"),
                            removed => done(removed.map(|r| r.map(drop))),
                        };
                    }
                    let bytes = match Limited::new(request.into_body(), MAX_FILE).collect().await {
                        Ok(collected) => collected.to_bytes(),
                        Err(_) => return said(StatusCode::PAYLOAD_TOO_LARGE, "a file of at most 256 MB"),
                    };
                    let shared = self.clone();
                    done(tokio::task::spawn_blocking(move || shared.write(&rel, &bytes, by)).await)
                }
                _ => said(StatusCode::METHOD_NOT_ALLOWED, "GET, HEAD, PUT, or DELETE"),
            };
        }
        if method != Method::GET && method != Method::HEAD {
            return said(StatusCode::METHOD_NOT_ALLOWED, "GET or HEAD");
        }
        match path.as_str() {
            "/" => moved("/index.html?bundle=/bundle/&serve"),
            "/edit" => moved("/editor.html?bundle=/bundle/&serve"),
            "/scaena/events" => self.events(),
            "/scaena/state" => {
                let json = Bytes::from(self.status().to_string());
                answer(StatusCode::OK, "application/json", json, "no-store")
            }
            _ => page(path.trim_start_matches('/'), request.headers(), method == Method::HEAD),
        }
    }

    /// Whether the header `name` names this server, after `scheme`. A write's `Origin` may be
    /// missing, as it is from a client that is no page.
    fn ours(&self, headers: &HeaderMap, name: header::HeaderName, scheme: &str) -> bool {
        match headers.get(&name) {
            Some(value) => names_us(value.as_bytes(), scheme, self.port),
            None => name == header::ORIGIN,
        }
    }

    /// The bundle's file at `rel`, as it is on disk now.
    async fn read(self: Arc<Self>, rel: &str, head: bool) -> Response<Body> {
        let shared = self.clone();
        let owned = rel.to_string();
        let read = tokio::task::spawn_blocking(move || {
            shared.existing(&owned).and_then(|p| std::fs::read(p).map_err(|e| e.to_string()))
        })
        .await;
        match read {
            Ok(Ok(bytes)) => {
                let len = bytes.len();
                let mut response = answer(
                    StatusCode::OK,
                    pages::media_type(rel),
                    if head { Bytes::new() } else { bytes.into() },
                    "no-store",
                );
                if head {
                    response.headers_mut().insert(header::CONTENT_LENGTH, len.into());
                }
                response
            }
            Ok(Err(e)) => said(StatusCode::NOT_FOUND, &e),
            Err(_) => said(StatusCode::INTERNAL_SERVER_ERROR, "reading the file stopped"),
        }
    }

    /// Server-sent events: `hello` with where the deck stands, then `changed` and `failed` as
    /// they happen, and a comment while there is nothing to say.
    fn events(&self) -> Response<Body> {
        let mut heard = self.events.subscribe();
        let hello = event("hello", &self.status());
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            if tx.send(hello).await.is_err() {
                return;
            }
            let mut quiet = tokio::time::interval(KEEP_ALIVE);
            quiet.tick().await;
            loop {
                let next = tokio::select! {
                    heard = heard.recv() => match heard {
                        Ok(event) => event,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return,
                    },
                    _ = quiet.tick() => Bytes::from_static(b": still here\n\n"),
                };
                // The page went away.
                if tx.send(next).await.is_err() {
                    return;
                }
            }
        });
        let mut response = Response::new(Events(rx).boxed());
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }
}

/// One server-sent event.
pub(crate) fn event(name: &str, data: &serde_json::Value) -> Bytes {
    Bytes::from(format!("event: {name}\ndata: {data}\n\n"))
}

/// A stream of events, as a response's body, until the page goes away.
struct Events(mpsc::Receiver<Bytes>);

impl hyper::body::Body for Events {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.0.poll_recv(cx).map(|event| event.map(|bytes| Ok(Frame::data(bytes))))
    }
}

/// Whether `value` is `localhost` or 127.0.0.1 at `port`, after `scheme`: at port 80 without the
/// port too, as a browser says it there.
fn names_us(value: &[u8], scheme: &str, port: u16) -> bool {
    ["localhost", "127.0.0.1"].iter().any(|host| {
        let at = format!("{scheme}{host}:{port}");
        value.eq_ignore_ascii_case(at.as_bytes())
            || (port == 80 && value.eq_ignore_ascii_case(format!("{scheme}{host}").as_bytes()))
    })
}

/// A path inside the bundle, from the request's (percent-encoded): `/`-separated names, none
/// empty, none `.` or `..`, none starting with a dot.
fn bundle_path(encoded: &str) -> Option<String> {
    let decoded = percent_decoded(encoded)?;
    let parts: Vec<&str> = decoded.split('/').collect();
    let fine = |p: &&str| !p.is_empty() && !p.starts_with('.') && !p.contains(['\\', '\0', ':']);
    parts.iter().all(fine).then_some(decoded)
}

fn percent_decoded(s: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut rest = s.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            // Two hex digits, and nothing `from_str_radix` would take besides (`%+1`).
            let hex = tail.get(..2).filter(|h| h.iter().all(u8::is_ascii_hexdigit))?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(b);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// The page's file at `rel`, gzipped where the client takes gzip, as every browser does.
fn page(rel: &str, headers: &HeaderMap, head: bool) -> Response<Body> {
    let Some(gzipped) = pages::gzipped(rel) else {
        return said(StatusCode::NOT_FOUND, "no such page");
    };
    let gzip = headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.split(',').any(|e| e.trim().split(';').next() == Some("gzip")));
    let bytes = if gzip { Bytes::from_static(gzipped) } else { gunzipped(gzipped).into() };
    let len = bytes.len();
    let mut response =
        answer(StatusCode::OK, pages::media_type(rel), if head { Bytes::new() } else { bytes }, "no-cache");
    if gzip {
        response.headers_mut().insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    if head {
        response.headers_mut().insert(header::CONTENT_LENGTH, len.into());
    }
    response
}

fn gunzipped(gzipped: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(gzipped), &mut out)
        .expect("a page gzipped by the build");
    out
}

fn answer(status: StatusCode, media: &str, bytes: Bytes, cache: &'static str) -> Response<Body> {
    let mut response = Response::new(Full::new(bytes).boxed());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(media).expect("a media type"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

fn said(status: StatusCode, message: &str) -> Response<Body> {
    answer(status, "text/plain; charset=utf-8", Bytes::from(format!("{message}\n")), "no-store")
}

fn moved(to: &'static str) -> Response<Body> {
    let mut response = said(StatusCode::FOUND, to);
    response.headers_mut().insert(header::LOCATION, HeaderValue::from_static(to));
    response
}

/// A write's answer: 204, or why not.
fn done(result: Result<Result<(), String>, tokio::task::JoinError>) -> Response<Body> {
    match result {
        Ok(Ok(())) => {
            let mut response = said(StatusCode::NO_CONTENT, "");
            *response.body_mut() = Full::new(Bytes::new()).boxed();
            response
        }
        Ok(Err(e)) => said(StatusCode::BAD_REQUEST, &e),
        Err(_) => said(StatusCode::INTERNAL_SERVER_ERROR, "the write stopped"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_path_stays_inside_the_bundle() {
        assert_eq!(bundle_path("deck.json").as_deref(), Some("deck.json"));
        assert_eq!(bundle_path("assets/my%20photo.png").as_deref(), Some("assets/my photo.png"));
        for outside in [
            "",
            "../x",
            "a/../b",
            "a//b",
            "/etc/passwd",
            ".git/config",
            "a/.hidden",
            "%2e%2e/x",
            "a%2F..%2Fb/..",
            "c:x",
            "a\\b",
            "%zz",
            "%+1",
            "%-1",
            "%e2%28",
        ] {
            assert_eq!(bundle_path(outside), None, "{outside:?}");
        }
        // An encoded slash is a slash: no name of its own can hide a `..`.
        assert_eq!(bundle_path("a%2Fb").as_deref(), Some("a/b"));
        assert_eq!(bundle_path("a%2F..%2Fb"), None);
    }

    #[test]
    fn only_this_machines_names_are_ours() {
        assert!(names_us(b"localhost:4848", "", 4848));
        assert!(names_us(b"LocalHost:4848", "", 4848));
        assert!(names_us(b"127.0.0.1:4848", "", 4848));
        assert!(names_us(b"http://localhost:4848", "http://", 4848));
        // A browser leaves out port 80, from `Host` and `Origin` alike.
        assert!(names_us(b"localhost", "", 80));
        assert!(names_us(b"http://127.0.0.1", "http://", 80));
        for host in ["localhost", "localhost:4849", "localhost.:4848", "attacker.example:4848", "[::1]:4848"] {
            assert!(!names_us(host.as_bytes(), "", 4848), "{host}");
        }
        for origin in ["null", "http://localhost", "https://localhost:4848", "http://attacker.example:4848"] {
            assert!(!names_us(origin.as_bytes(), "http://", 4848), "{origin}");
        }
    }
}
