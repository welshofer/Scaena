//! `scaena serve` (PLAN 2.11, ADR-0012) over a socket, as a page meets it:
//! - the bundle's files are the folder's;
//! - a page of the server's own writes inside the folder only, and every page hears of it once;
//! - a request that names another host, or a write from another origin, is refused;
//! - a `deck.scn` saved on disk compiles into `deck.json` and is announced;
//! - one that does not compile is announced at its line, and the deck is kept.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use scaena_serve::{Note, Serve, ServeError};
use serde_json::Value;

const EXAMPLES: &str = "../../docs/examples";

/// The revenue example as a bundle's folder: `deck.json`, its source as `deck.scn`, and the
/// files they name.
fn bundle(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("serve-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        ("revenue.deck.json", "deck.json"),
        ("revenue.deck.scn", "deck.scn"),
        ("themes/dusk.theme.json", "themes/dusk.theme.json"),
        ("fonts/Fraunces-VF.ttf", "fonts/Fraunces-VF.ttf"),
        ("fonts/Inter-VF.ttf", "fonts/Inter-VF.ttf"),
        ("fonts/JetBrainsMono-VF.ttf", "fonts/JetBrainsMono-VF.ttf"),
        ("fonts/Fraunces-Italic-VF.ttf", "fonts/Fraunces-Italic-VF.ttf"),
        ("fonts/Inter-Italic-VF.ttf", "fonts/Inter-Italic-VF.ttf"),
        ("fonts/JetBrainsMono-Italic-VF.ttf", "fonts/JetBrainsMono-Italic-VF.ttf"),
        ("data/q3-revenue.csv", "data/q3-revenue.csv"),
    ];
    for (from, to) in files {
        std::fs::create_dir_all(dir.join(to).parent().unwrap()).unwrap();
        std::fs::copy(Path::new(EXAMPLES).join(from), dir.join(to)).unwrap();
    }
    dir
}

/// A server on `dir`, at a free port, serving on a thread of its own; and what it notes.
fn serve(dir: &Path) -> (SocketAddr, tokio::sync::broadcast::Receiver<Note>) {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let serve = runtime.block_on(Serve::bind(dir, 0)).unwrap();
    let (addr, notes) = (serve.addr(), serve.notes());
    std::thread::spawn(move || runtime.block_on(serve.run()));
    (addr, notes)
}

struct Answer {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        let prefix = format!("{}:", name.to_ascii_lowercase());
        self.head.lines().find_map(|l| l.to_ascii_lowercase().starts_with(&prefix).then(|| l[prefix.len()..].trim()))
    }
}

/// One request, as `head` (its line and headers, without the blank line) and `body`.
fn request(addr: SocketAddr, head: &str, body: &[u8]) -> Answer {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let length = if body.is_empty() { String::new() } else { format!("Content-Length: {}\r\n", body.len()) };
    stream.write_all(format!("{head}\r\nConnection: close\r\n{length}\r\n").as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("an answer's head");
    let head = String::from_utf8(raw[..split].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Answer { status, head, body: raw[split + 4..].to_vec() }
}

fn get(addr: SocketAddr, path: &str) -> Answer {
    request(addr, &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}", addr.port()), b"")
}

/// A write as a page of the server's own makes it: its origin, and its id.
fn write(addr: SocketAddr, method: &str, path: &str, body: &[u8], origin: &str) -> Answer {
    let port = addr.port();
    let head =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nOrigin: {origin}\r\nX-Scaena-Client: page-1");
    request(addr, &head, body)
}

/// The events a page hears, read as it hears them.
struct Events(BufReader<TcpStream>);

impl Events {
    fn open(addr: SocketAddr) -> Events {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        // HTTP/1.0, so the stream comes as it is, not in chunks.
        stream
            .write_all(format!("GET /scaena/events HTTP/1.0\r\nHost: 127.0.0.1:{}\r\n\r\n", addr.port()).as_bytes())
            .unwrap();
        let mut events = Events(BufReader::new(stream));
        let deadline = Instant::now() + Duration::from_secs(20);
        while !events.line(deadline).expect("the events' head").is_empty() {}
        events
    }

    fn line(&mut self, deadline: Instant) -> Option<String> {
        let mut line = String::new();
        loop {
            match self.0.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) if line.ends_with('\n') => return Some(line.trim_end_matches(['\r', '\n']).to_string()),
                Ok(_) => {}
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                    if Instant::now() > deadline {
                        return None;
                    }
                }
                Err(e) => panic!("{e}"),
            }
        }
    }

    /// The next event named `name`, within 20 s, passing over any other.
    fn next(&mut self, name: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        let (mut event, mut data) = (String::new(), String::new());
        loop {
            let line = self.line(deadline).unwrap_or_else(|| panic!("no `{name}` event"));
            if let Some(e) = line.strip_prefix("event: ") {
                event = e.to_string();
            } else if let Some(d) = line.strip_prefix("data: ") {
                data = d.to_string();
            } else if line.is_empty() && !event.is_empty() {
                if event == name {
                    return serde_json::from_str(&data).unwrap();
                }
                event.clear();
            }
        }
    }
}

/// The next note that `wanted` picks, within 20 s.
fn noted<T>(notes: &mut tokio::sync::broadcast::Receiver<Note>, wanted: impl Fn(Note) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        match notes.try_recv() {
            Ok(note) => {
                if let Some(t) = wanted(note) {
                    return t;
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    panic!("not noted");
}

#[test]
fn the_bundle_is_the_folders_and_nothing_outside_it() {
    let dir = bundle("files");
    let (addr, _) = serve(&dir);
    let deck = get(addr, "/bundle/deck.json");
    assert_eq!(deck.status, 200);
    assert_eq!(deck.body, std::fs::read(dir.join("deck.json")).unwrap());
    assert_eq!(deck.header("content-type"), Some("application/json"));
    assert_eq!(deck.header("cache-control"), Some("no-store"));
    assert_eq!(get(addr, "/bundle/fonts/Inter-VF.ttf").header("content-type"), Some("font/ttf"));
    assert_eq!(get(addr, "/bundle/nothing.json").status, 404);
    for outside in ["/bundle/../Cargo.toml", "/bundle/%2e%2e/Cargo.toml", "/bundle/fonts/../../x", "/bundle/.hidden"] {
        assert_eq!(get(addr, outside).status, 400, "{outside}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"), dir.join("out.toml"))
            .unwrap();
        assert_eq!(get(addr, "/bundle/out.toml").status, 404, "a link out of the folder is not the bundle's");
    }
    // A request through another name for this machine is refused: no other site's page can
    // reach the server by resolving a name of its own to it.
    let elsewhere = request(addr, "GET /bundle/deck.json HTTP/1.1\r\nHost: attacker.example", b"");
    assert_eq!(elsewhere.status, 403);
    // The player and the editor, on the bundle.
    let player = get(addr, "/");
    assert_eq!((player.status, player.header("location")), (302, Some("/index.html?bundle=/bundle/&serve")));
    assert_eq!(get(addr, "/edit").header("location"), Some("/editor.html?bundle=/bundle/&serve"));
    let state: Value = serde_json::from_slice(&get(addr, "/scaena/state").body).unwrap();
    assert_eq!(state, serde_json::json!({ "version": 0, "name": "serve-files", "source": true, "failed": null }));
    // The pages, when this build carries them.
    let page = get(addr, "/index.html");
    if scaena_serve::pages_built() {
        assert_eq!(page.status, 200);
        assert_eq!(page.header("content-type"), Some("text/html; charset=utf-8"));
        assert!(String::from_utf8(page.body).unwrap().contains("<html"));
    } else {
        assert_eq!(page.status, 404);
    }
}

#[test]
fn a_page_writes_inside_the_folder_and_is_heard_once() {
    let dir = bundle("writes");
    let (addr, _) = serve(&dir);
    let ours = format!("http://localhost:{}", addr.port());
    let mut events = Events::open(addr);
    assert_eq!(events.next("hello")["version"], 0);

    let png = b"\x89PNG not really";
    assert_eq!(write(addr, "PUT", "/bundle/assets/new%20one.png", png, &ours).status, 204);
    assert_eq!(std::fs::read(dir.join("assets/new one.png")).unwrap(), png);
    let changed = events.next("changed");
    assert_eq!(changed["by"], "page-1");
    assert_eq!(changed["paths"], serde_json::json!(["assets/new one.png"]));
    assert_eq!(get(addr, "/bundle/assets/new%20one.png").body, png);

    // Another site's page cannot write here, nor through `..`.
    assert_eq!(write(addr, "PUT", "/bundle/assets/evil.png", png, "http://attacker.example").status, 403);
    assert!(!dir.join("assets/evil.png").exists());
    assert_eq!(write(addr, "PUT", "/bundle/../evil.png", png, &ours).status, 400);
    assert!(!dir.parent().unwrap().join("evil.png").exists());
    assert_eq!(write(addr, "PUT", "/bundle/fonts", png, &ours).status, 400, "not over a folder");

    assert_eq!(write(addr, "DELETE", "/bundle/assets/new%20one.png", b"", &ours).status, 204);
    assert!(!dir.join("assets/new one.png").exists());
    let changed = events.next("changed");
    assert_eq!((changed["by"].as_str(), changed["version"].as_u64()), (Some("page-1"), Some(2)));
    // A file that is not there is not found, which a page's save takes as removed.
    assert_eq!(write(addr, "DELETE", "/bundle/assets/new%20one.png", b"", &ours).status, 404);
    assert_eq!(write(addr, "DELETE", "/bundle/nowhere/x.png", b"", &ours).status, 404);
    assert_eq!(write(addr, "DELETE", "/bundle/fonts", b"", &ours).status, 400, "not a folder");

    #[cfg(unix)]
    {
        // A link is removed as a name in the folder; what it leads to stays.
        std::os::unix::fs::symlink(dir.join("deck.json"), dir.join("alias.json")).unwrap();
        assert_eq!(write(addr, "DELETE", "/bundle/alias.json", b"", &ours).status, 204);
        assert!(dir.join("deck.json").is_file() && dir.join("alias.json").symlink_metadata().is_err());
        // Nothing is written, nor any folder made, through a link that leads out of the folder.
        let outside = Path::new(env!("CARGO_TARGET_TMPDIR")).join("serve-writes-outside");
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("out")).unwrap();
        assert_eq!(write(addr, "PUT", "/bundle/out/x.png", png, &ours).status, 400);
        assert_eq!(write(addr, "PUT", "/bundle/out/made/x.png", png, &ours).status, 400);
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "nothing outside the folder");
    }
}

#[test]
fn a_source_saved_on_disk_compiles_into_the_deck_and_a_broken_one_keeps_it() {
    let dir = bundle("source");
    let (addr, mut notes) = serve(&dir);
    let mut events = Events::open(addr);
    events.next("hello");
    let source = std::fs::read_to_string(dir.join("deck.scn")).unwrap();
    assert!(source.contains("\"Q3 Review\""));

    // Saved in a text editor: the deck follows.
    std::fs::write(dir.join("deck.scn"), source.replace("\"Q3 Review\"", "\"Q4 Review\"")).unwrap();
    let changed = events.next("changed");
    assert_eq!(changed["by"], Value::Null);
    assert_eq!(changed["paths"], serde_json::json!(["deck.json", "deck.scn"]));
    assert_eq!(changed["failed"], Value::Null);
    let deck = std::fs::read_to_string(dir.join("deck.json")).unwrap();
    assert!(deck.contains("\"Q4 Review\"") && !deck.contains("\"Q3 Review\""));
    // Compiled once as the server started (to the same deck), and once for this.
    noted(&mut notes, |n| matches!(n, Note::Compiled { written: true, .. }).then_some(()));

    // A source that does not compile is said at its line, and the deck stays as it was.
    let broken = format!("{source}\nstate broken\n  t text \"unterminated\n");
    let line = broken.lines().position(|l| l.contains("unterminated")).unwrap() + 1;
    std::fs::write(dir.join("deck.scn"), &broken).unwrap();
    let failed = events.next("failed");
    let problem = &failed["failed"]["problems"][0];
    assert_eq!(problem["message"], "this string is not closed on its line");
    assert_eq!(problem["line"], line);
    assert_eq!(std::fs::read_to_string(dir.join("deck.json")).unwrap(), deck);
    let failure = noted(&mut notes, |n| match n {
        Note::Failed(f) => Some(f),
        _ => None,
    });
    assert_eq!(failure.source, broken);
    assert!(failure.problems[0].span.is_some());
    let state: Value = serde_json::from_slice(&get(addr, "/scaena/state").body).unwrap();
    assert_eq!(state["failed"]["problems"][0]["line"], line);

    // Mended, it compiles again, and the failure is over.
    std::fs::write(dir.join("deck.scn"), &source).unwrap();
    let changed = events.next("changed");
    assert_eq!(changed["failed"], Value::Null);
    assert!(std::fs::read_to_string(dir.join("deck.json")).unwrap().contains("\"Q3 Review\""));
}

#[test]
fn a_folder_without_a_deck_is_not_served() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("serve-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    assert!(matches!(runtime.block_on(Serve::bind(&dir, 0)), Err(ServeError::NotABundle(_))));
    assert!(matches!(runtime.block_on(Serve::bind(&dir.join("nowhere"), 0)), Err(ServeError::NotABundle(_))));
}
