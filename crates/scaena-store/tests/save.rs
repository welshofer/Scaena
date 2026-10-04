//! Saving bundles (PLAN 1.4): in place, from a bare deck file, and what a saved bundle holds.

use scaena_core::validate::validate_bundle;
use scaena_store::{Bundle, SaveOptions};
use std::path::{Path, PathBuf};

const NOW: &str = "2026-10-02T00:00:00Z";

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("store-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() { copy_dir(&path, &target) } else { std::fs::copy(&path, &target).map(|_| ()).unwrap() }
    }
}

fn opts() -> SaveOptions {
    SaveOptions { subset_fonts: true, now: NOW.into(), history: false }
}

/// Every file in `dir`, relative, sorted.
fn files(dir: &Path) -> Vec<String> {
    let bundle = Bundle::open(dir).unwrap();
    bundle.files.list().unwrap()
}

#[test]
fn saving_in_place_renames_fonts_and_removes_the_old_files() {
    let dir = scratch("in-place").join("b1.scaena");
    copy_dir(Path::new("../../tests/bench/b1.scaena"), &dir);
    let saved = Bundle::open(&dir).unwrap().save(&dir, &opts()).unwrap();
    assert_eq!(saved.renamed.len(), 4, "{:?}", saved.renamed);
    let after = files(&dir);
    for (old, new) in &saved.renamed {
        assert!(!after.contains(old), "{old} is gone");
        assert!(after.contains(new), "{new} is there");
    }
    assert!(after.contains(&"manifest.json".to_string()));
    assert!(after.contains(&"fonts/OFL-Inter.txt".to_string()), "licenses travel with their fonts");
    let (deck, files) = scaena_store::open_unparsed(&dir).unwrap();
    assert_eq!(validate_bundle(&deck, &files).unwrap(), [], "the saved bundle validates");
}

#[test]
fn a_bare_deck_saves_with_what_it_references_and_no_more() {
    let dir = scratch("bare").join("revenue");
    let saved = Bundle::open(Path::new("../../docs/examples/revenue.deck.json")).unwrap().save(&dir, &opts()).unwrap();
    let after = files(&dir);
    assert!(
        after.iter().all(|f| {
            f == "deck.json"
                || f == "manifest.json"
                || f == "themes/dusk.theme.json"
                || f.starts_with("data/")
                || f.starts_with("fonts/")
        }),
        "{after:#?}"
    );
    assert!(!after.iter().any(|f| f.starts_with("authorability/")), "another bundle's files stay where they are");
    assert_eq!(saved.subset.len(), 3);
    let (deck, files) = scaena_store::open_unparsed(&dir).unwrap();
    assert_eq!(validate_bundle(&deck, &files).unwrap(), []);
}

#[test]
fn a_beat_cites_an_image_by_the_name_saving_gives_it() {
    let dir = scratch("evidence").join("trails");
    let saved = Bundle::open(Path::new("../../docs/examples/trails.deck.json")).unwrap().save(&dir, &opts()).unwrap();
    let (_, new) =
        saved.renamed.iter().find(|(old, _)| old == "assets/trails-ridge.png").expect("the photo is renamed");
    let deck: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
    let cited: Vec<&str> = deck["spine"]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["beats"].as_array().unwrap())
        .flat_map(|b| b["evidence"].as_array().into_iter().flatten())
        .filter_map(|e| e.as_str().filter(|e| !e.starts_with('@')))
        .collect();
    assert_eq!(cited, [new.as_str()], "the saddle beat cites the photo where it now is");
    assert!(dir.join(new).exists());
}

#[test]
fn a_directory_that_holds_something_else_is_not_overwritten() {
    let dir = scratch("occupied");
    std::fs::write(dir.join("notes.txt"), "mine").unwrap();
    let err = Bundle::open(Path::new("../../tests/bench/b1.scaena")).unwrap().save(&dir, &opts()).unwrap_err();
    assert!(err.to_string().contains("will not save over"), "{err}");
    assert_eq!(std::fs::read_to_string(dir.join("notes.txt")).unwrap(), "mine");
}

#[test]
fn keeping_fonts_whole_keeps_their_bytes() {
    let dir = scratch("whole").join("b1");
    let opts = SaveOptions { subset_fonts: false, now: NOW.into(), history: false };
    let saved = Bundle::open(Path::new("../../tests/bench/b1.scaena")).unwrap().save(&dir, &opts).unwrap();
    assert!(saved.subset.is_empty());
    for (old, new) in &saved.renamed {
        let before = std::fs::read(Path::new("../../tests/bench/b1.scaena").join(old)).unwrap();
        assert_eq!(std::fs::read(dir.join(new)).unwrap(), before, "{old}");
    }
}

/// Every file under `dir`, by its path inside it.
fn read_all(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let bundle = Bundle::open(dir).unwrap();
    bundle.files.list().unwrap().into_iter().map(|rel| (rel.clone(), bundle.read(&rel).unwrap())).collect()
}

#[test]
fn a_bundle_in_memory_saves_as_it_does_on_disk() {
    // A page holds a bundle's files (PLAN 2.4): saved in memory, it is what a save writes.
    let dir = Path::new("../../tests/bench/b1.scaena");
    let on_disk = scratch("memory-disk").join("b1.scaena");
    Bundle::open(dir).unwrap().save(&on_disk, &opts()).unwrap();
    let in_memory = Bundle::in_memory(read_all(dir)).unwrap().saving(&opts()).unwrap();
    assert_eq!(in_memory.files, read_all(&on_disk));
    assert_eq!(in_memory.saved.renamed.len(), 4, "{:?}", in_memory.saved.renamed);
    // Zipped, the same files make the same bytes, and open again as they were.
    let zipped = scaena_store::zip(&in_memory.files).unwrap();
    assert_eq!(zipped, scaena_store::zip(&in_memory.files).unwrap());
    let reopened = Bundle::from_zip(&zipped).unwrap();
    assert_eq!(reopened.deck.to_json().unwrap(), Bundle::open(&on_disk).unwrap().deck.to_json().unwrap());
    assert_eq!(reopened.files.list().unwrap(), in_memory.files.keys().cloned().collect::<Vec<_>>());
}

#[test]
fn a_bundle_in_memory_keeps_its_paths_inside_it() {
    let mut files = read_all(Path::new("../../tests/bench/b1.scaena"));
    files.insert("../outside.txt".into(), b"no".to_vec());
    assert!(Bundle::in_memory(files).is_err());
}

#[test]
fn a_file_added_to_a_bundle_goes_where_its_kind_goes() {
    let png = b"\x89PNG\r\n\x1a\nnot really";
    let image = scaena_store::place("Photo Of Me.PNG", png);
    assert!(
        image.starts_with("assets/") && image.ends_with(".png") && image.len() == "assets/".len() + 64 + 4,
        "{image}"
    );
    assert_eq!(image, scaena_store::place("other-name.png", png), "named by its content");
    assert_eq!(scaena_store::place("dir/Inter-VF.ttf", b"font"), "fonts/Inter-VF.ttf");
    assert_eq!(scaena_store::place("q3.csv", b"a,b"), "data/q3.csv");
}
