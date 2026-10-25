//! `scaena save` end to end (PLAN 1.4): a saved bundle draws every frame the original drew.
//! Fonts are subset (glyph ids kept) and renamed by their content, and the bundle zips to
//! the same bytes every time.

use scaena_core::displaylist::{DisplayList, quantize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TORTURE: &str = "../../tests/fixtures/torture.scaena";
/// `sha256` of `assets/test-card.png`, the name saving gives it.
const TEST_CARD_SHA256: &str = "7130f10a4aa0167a731f7735296b6079436f52e077fd5f2ea988d8c180ac2923";
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
    assert_eq!(renamed.len(), 15, "every font and image is renamed by its content: {renamed:?}");
    let fonts = renamed.iter().filter(|(_, new)| new.starts_with("fonts/") && new.ends_with(".ttf")).count();
    let image = ("assets/test-card.png".to_string(), format!("assets/{}.png", TEST_CARD_SHA256));
    // The seven photos keep what they are: JPEGs, each named by its bytes (PLAN 2.66).
    let photos = renamed.iter().filter(|(old, new)| old.ends_with(".jpg") && new.ends_with(".jpg")).count();
    assert_eq!((fonts, renamed.contains(&image), photos), (6, true, 7), "{renamed:?}");
    // The scripts and features most likely to break in a subset: joining, bidi, marks,
    // color glyphs, ligatures, kerning, hanging quotes, variable axes, code's tabs and the mark
    // that runs it left to right; the images, found under their new names; and a map's label and
    // credit, which the subset keeps the characters of (ADR-0030).
    for state in [
        "bidi-arabic",
        "bidi-hebrew",
        "combining",
        "emoji",
        "liga",
        "dlig",
        "kern",
        "hanging",
        "axes",
        "accents",
        "code",
        "images",
        "maps",
    ] {
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

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() { copy_dir(&path, &target) } else { std::fs::copy(&path, &target).map(|_| ()).unwrap() }
    }
}

/// `scaena save` never subsets a family its theme keeps whole (ADR-0031, call 5), as Hanji's
/// Pretendard will be: B1 with Bench Serif so marked keeps its bytes, and the rest are subset.
#[test]
fn scaena_save_keeps_a_family_its_theme_keeps_whole() {
    let dir = scratch("whole");
    let marked = dir.join("b1");
    copy_dir(Path::new(B1), &marked);
    let theme = std::fs::read_to_string(marked.join("theme.json")).unwrap();
    let edited = theme.replacen(r#""family": "Bench Serif","#, r#""family": "Bench Serif", "whole": true,"#, 1);
    assert_ne!(edited, theme);
    std::fs::write(marked.join("theme.json"), edited).unwrap();
    let out = scaena(&["--json", "save", marked.to_str().unwrap(), "--to", dir.join("saved").to_str().unwrap()]);
    let saved: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let subset: Vec<&str> = saved["subset"].as_array().unwrap().iter().map(|s| s[0].as_str().unwrap()).collect();
    assert_eq!(subset, ["fonts/Fraunces-VF.ttf", "fonts/Inter-VF.ttf", "fonts/JetBrainsMono-VF.ttf"]);
    let renamed = saved["renamed"].as_array().unwrap();
    let new = renamed.iter().find(|r| r[0] == "fonts/BenchSerif-VF.ttf").unwrap()[1].as_str().unwrap();
    assert_eq!(
        std::fs::read(dir.join("saved").join(new)).unwrap(),
        std::fs::read(format!("{B1}/fonts/BenchSerif-VF.ttf")).unwrap(),
        "kept whole, byte for byte"
    );
}

/// A save the system refuses says which file and why, each once (lift-up 10-24b USE-3): onto a
/// path through a file, where a folder goes.
#[test]
fn a_save_the_system_refuses_says_where_and_why_once() {
    let dir = scratch("refused");
    let blocked = dir.join("a-file");
    std::fs::write(&blocked, b"not a folder").unwrap();
    let to = blocked.join("out");
    let out = Command::new(env!("CARGO_BIN_EXE_scaena"))
        .args(["save", "../../tests/bench/b1.scaena", "--to", to.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "a save through a file fails");
    let said = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(said.contains(&format!("writing {}", to.display()).to_ascii_lowercase()), "{said}");
    assert_eq!(said.matches("not a directory").count(), 1, "the reason once: {said}");
}
