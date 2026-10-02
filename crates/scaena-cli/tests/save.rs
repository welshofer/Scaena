//! `scaena save` end to end (PLAN 1.4): a saved bundle draws every frame the original drew.
//! Fonts are subset (glyph ids kept) and renamed by their content, and the bundle zips to
//! the same bytes every time.

use scaena_core::displaylist::{DisplayList, quantize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TORTURE: &str = "../../tests/fixtures/torture.scaena";
const B1: &str = "../../tests/bench/b1.scaena";
const GOLDEN: &str = "../../tests/golden/torture";

fn scaena(args: &[&str]) -> Output {
    let out =
        Command::new(env!("CARGO_BIN_EXE_scaena")).env("SOURCE_DATE_EPOCH", "1790000000").args(args).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    out
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("save-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `bundle` saved to `to`, and its renames, (old path, new path).
fn save(bundle: &str, to: &Path) -> Vec<(String, String)> {
    let out = scaena(&["--json", "save", bundle, "--to", to.to_str().unwrap()]);
    let saved: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    saved["renamed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r[0].as_str().unwrap().into(), r[1].as_str().unwrap().into()))
        .collect()
}

/// The display list `bundle` draws for `state` at rest, quantized, as golden JSON, with
/// font ids put back to their names before the save.
fn frame(bundle: &Path, state: &str, dir: &Path, renamed: &[(String, String)]) -> String {
    let (png, dl) = (dir.join(format!("{state}.png")), dir.join(format!("{state}.dl.json")));
    scaena(&[
        "render",
        bundle.to_str().unwrap(),
        "--state",
        state,
        "--out",
        png.to_str().unwrap(),
        "--display-list",
        dl.to_str().unwrap(),
    ]);
    let mut list = DisplayList::from_json(&std::fs::read_to_string(&dl).unwrap()).unwrap();
    quantize(&mut list);
    let mut json = list.to_golden_json().unwrap();
    for (old, new) in renamed {
        json = json.replace(new.as_str(), old.as_str());
    }
    json
}

#[test]
fn a_saved_torture_deck_draws_its_golden_frames() {
    let dir = scratch("torture");
    let zip = dir.join("torture.scaena");
    let renamed = save(TORTURE, &zip);
    assert_eq!(renamed.len(), 5, "every font is renamed by its content: {renamed:?}");
    assert!(renamed.iter().all(|(_, new)| new.starts_with("fonts/") && new.ends_with(".ttf")));
    // The scripts and features most likely to break in a subset: joining, bidi, marks,
    // color glyphs, ligatures, kerning, hanging quotes, variable axes.
    for state in
        ["bidi-arabic", "bidi-hebrew", "combining", "emoji", "liga", "dlig", "kern", "hanging", "axes", "accents"]
    {
        let golden = std::fs::read_to_string(format!("{GOLDEN}/{state}.dl.json")).unwrap();
        assert_eq!(frame(&zip, state, &dir, &renamed), golden, "{state}");
    }
}

#[test]
fn a_saved_benchmark_deck_draws_what_it_drew_before() {
    let dir = scratch("b1");
    let saved = dir.join("b1");
    let renamed = save(B1, &saved);
    let (before, after) = (dir.join("before"), dir.join("after"));
    std::fs::create_dir_all(&before).unwrap();
    std::fs::create_dir_all(&after).unwrap();
    let deck: serde_json::Value = serde_json::from_slice(&std::fs::read(format!("{B1}/deck.json")).unwrap()).unwrap();
    for state in deck["states"].as_array().unwrap().iter().step_by(8).map(|s| s["id"].as_str().unwrap()) {
        assert_eq!(frame(&saved, state, &after, &renamed), frame(Path::new(B1), state, &before, &[]), "{state}");
    }
}

#[test]
fn saving_twice_writes_the_same_bytes_and_a_true_manifest() {
    let dir = scratch("twice");
    let (a, b) = (dir.join("a.scaena"), dir.join("b.scaena"));
    save(TORTURE, &a);
    save(TORTURE, &b);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());

    // Unzipped through the CLI's own reader: every file the manifest lists has its hash.
    let unzipped = dir.join("unzipped");
    save(a.to_str().unwrap(), &unzipped);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(unzipped.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["created"], "2026-09-21T14:13:20Z", "SOURCE_DATE_EPOCH, in RFC 3339");
    use sha2::Digest;
    let hash = |bytes: &[u8]| sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(manifest["deck"], hash(&std::fs::read(unzipped.join("deck.json")).unwrap()));
    for (path, sha) in manifest["files"].as_object().unwrap() {
        assert_eq!(sha.as_str().unwrap(), hash(&std::fs::read(unzipped.join(path)).unwrap()), "{path}");
    }
    // A zip validates like a directory.
    scaena(&["validate", a.to_str().unwrap()]);
    // A saved bundle saves to itself: names by content are fixed points.
    let again = dir.join("again");
    assert_eq!(save(unzipped.to_str().unwrap(), &again), Vec::<(String, String)>::new());
}
