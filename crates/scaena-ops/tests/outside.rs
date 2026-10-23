//! An export names files after a deck's ids, and a bundle nobody validated may hold any id: one
//! that is no id is refused, naming it, and nothing is written outside `--out` (lift-up SEC-3).

use scaena_ops::export::{Request, export};
use std::path::{Path, PathBuf};

const REVENUE: &str = "../../docs/examples/revenue.deck.json";

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn an_id_that_would_write_outside_out_is_refused() {
    let root = scratch("outside");
    let out = root.join("a/b");
    for (format, id) in [("png", "../../escaped"), ("svg", "/tmp/escaped"), ("png", "Close")] {
        let mut b = scaena_ops::open(Path::new(REVENUE)).unwrap();
        b.deck.states.last_mut().unwrap().id = id.into();
        let req = Request { format: format.into(), out: Some(out.clone()), ..Request::default() };
        let e = export(&b, &req).unwrap_err();
        assert!(e.message.contains(&format!("the state `{id}` is no id")), "{e:?}");
    }
    let mut b = scaena_ops::open(Path::new(REVENUE)).unwrap();
    b.deck.spine.as_mut().unwrap().sections[0].beats[0].id = "../../escaped".into();
    let req = Request { format: "spine".into(), out: Some(out.join("spine.json")), ..Request::default() };
    let e = export(&b, &req).unwrap_err();
    assert!(e.message.contains("the beat `../../escaped` is no id"), "{e:?}");
    let mut written = Vec::new();
    let mut walk = vec![root.clone()];
    while let Some(dir) = walk.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            if entry.path().is_dir() { walk.push(entry.path()) } else { written.push(entry.path()) }
        }
    }
    assert_eq!(written, Vec::<PathBuf>::new(), "nothing is written");
    assert!(!root.join("escaped.png").exists() && !Path::new("/tmp/escaped.svg").exists());
}
