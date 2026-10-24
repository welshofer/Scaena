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
    assert_eq!(saved.subset.len(), 6, "each family's face and its italic");
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

/// A family its theme keeps whole is never subset (ADR-0031, call 5): B1 with Bench Serif marked
/// so, as Hanji will mark Pretendard, whose license reserves its name. Bench Serif is Source
/// Serif 4 renamed, a family whose name was reserved; which family is marked does not matter to
/// the save, which reads only the mark.
#[test]
fn a_family_kept_whole_keeps_its_bytes_when_the_rest_are_subset() {
    let b1 = Path::new("../../tests/bench/b1.scaena");
    let base = scratch("kept-whole");
    let dir = base.join("b1");
    copy_dir(b1, &dir);
    let theme = std::fs::read_to_string(dir.join("theme.json")).unwrap();
    let marked = theme.replacen(r#""family": "Bench Serif","#, r#""family": "Bench Serif", "whole": true,"#, 1);
    assert_ne!(marked, theme);
    std::fs::write(dir.join("theme.json"), marked).unwrap();

    let bundle = Bundle::open(&dir).unwrap();
    let (_, subsetting) = bundle.subsetting().unwrap();
    assert!(!subsetting.contains(&"fonts/BenchSerif-VF.ttf".to_string()), "nothing to subset: {subsetting:?}");
    assert_eq!(subsetting.len(), 3, "{subsetting:?}");

    let out = base.join("saved");
    let saved = bundle.save(&out, &opts()).unwrap();
    let subset: Vec<&str> = saved.subset.iter().map(|(path, _, _)| path.as_str()).collect();
    assert_eq!(subset.len(), 3, "{subset:?}");
    assert!(!subset.contains(&"fonts/BenchSerif-VF.ttf"));
    for (old, new) in saved.renamed.iter().filter(|(old, _)| old.starts_with("fonts/")) {
        let before = std::fs::read(b1.join(old)).unwrap();
        let after = std::fs::read(out.join(new)).unwrap();
        if old == "fonts/BenchSerif-VF.ttf" {
            assert_eq!(after, before, "kept whole, byte for byte");
        } else {
            assert_ne!(after, before, "{old} is subset");
        }
    }
    // And again, from what it saved: the mark travels with the theme.
    let again = Bundle::open(&out).unwrap().saving(&opts()).unwrap();
    let whole = std::fs::read(b1.join("fonts/BenchSerif-VF.ttf")).unwrap();
    assert!(again.files.values().any(|bytes| *bytes == whole), "still whole after a second save");
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

/// A save that stops partway leaves a bundle that opens (lift-up REL-3). Each file is written
/// whole or not at all, every file's bytes written before any is put in place, and the deck's
/// files before `deck.json`: a font that cannot be put in place stops the save before the deck is
/// touched, so `deck.json` names no file that is not there; and a `deck.json` that cannot be
/// written leaves the bundle as it was, and no partial in its listing.
#[test]
fn a_save_that_stops_partway_leaves_the_deck_as_it_was() {
    let dir = scratch("partway").join("b1.scaena");
    copy_dir(Path::new("../../tests/bench/b1.scaena"), &dir);
    let bundle = Bundle::open(&dir).unwrap();
    let before = std::fs::read(dir.join("deck.json")).unwrap();
    // A font the save names anew, by its content: a directory stands where it would go, which
    // no file can be renamed over.
    let saving = bundle.saving(&opts()).unwrap();
    let font = saving.files.keys().find(|p| p.starts_with("fonts/") && !dir.join(p).exists()).unwrap();
    std::fs::create_dir_all(dir.join(font)).unwrap();
    let err = bundle.save(&dir, &opts()).unwrap_err();
    assert!(matches!(err, scaena_store::StoreError::Io(_)), "{err}");
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before, "the fonts come before deck.json");
    let (deck, held) = scaena_store::open_unparsed(&dir).unwrap();
    assert_eq!(validate_bundle(&deck, &held).unwrap(), [], "the bundle opens as it was");
    assert!(!dir.join(font).join("x").exists() && dir.join(font).is_dir(), "the font's place is still the directory");
    std::fs::remove_dir(dir.join(font)).unwrap();
    // The deck's own write stops: its partial cannot be made. Every file's bytes are written
    // before any is put in place, so nothing has changed, the theme among it, and no partial
    // is listed.
    let listed = files(&dir);
    let theme = std::fs::read(dir.join("theme.json")).unwrap();
    std::fs::create_dir(dir.join(".deck.json.partial")).unwrap();
    let err = bundle.save(&dir, &opts()).unwrap_err();
    assert!(matches!(err, scaena_store::StoreError::Io(_)), "{err}");
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before, "deck.json is as it was");
    assert_eq!(std::fs::read(dir.join("theme.json")).unwrap(), theme, "and the theme");
    assert_eq!(files(&dir), listed, "no file was put in place, and no partial is listed");
    let (deck, held) = scaena_store::open_unparsed(&dir).unwrap();
    assert_eq!(validate_bundle(&deck, &held).unwrap(), [], "the bundle opens as it was");
    std::fs::remove_dir(dir.join(".deck.json.partial")).unwrap();
    // Nothing in the way, the save goes through, and leaves no partial behind.
    bundle.save(&dir, &opts()).unwrap();
    assert_ne!(std::fs::read(dir.join("deck.json")).unwrap(), before, "saved in canonical form");
    assert!(!dir.join(".deck.json.partial").exists());
    let (deck, held) = scaena_store::open_unparsed(&dir).unwrap();
    assert_eq!(validate_bundle(&deck, &held).unwrap(), []);
}

/// A `.scaena` zip is written whole or not at all: one that cannot be written is left as it
/// was, and opens (lift-up REL-3).
#[test]
fn a_zip_that_cannot_be_written_is_left_as_it_was() {
    let dir = scratch("zip-partway");
    let zip = dir.join("b1.scaena");
    Bundle::open(Path::new("../../tests/bench/b1.scaena")).unwrap().save(&zip, &opts()).unwrap();
    let before = std::fs::read(&zip).unwrap();
    std::fs::create_dir(dir.join(".b1.scaena.partial")).unwrap();
    let err = Bundle::open(&zip).unwrap().save(&zip, &opts()).unwrap_err();
    assert!(matches!(err, scaena_store::StoreError::Io(_)), "{err}");
    assert_eq!(std::fs::read(&zip).unwrap(), before, "the zip is as it was");
    let (deck, files) = scaena_store::open_unparsed(&zip).unwrap();
    assert_eq!(validate_bundle(&deck, &files).unwrap(), []);
}

/// A write into the bundle where it is ([`Bundle::write`]) writes `deck.json` after the rest, each
/// whole or not at all: one that stops on another file leaves the deck as it was (lift-up REL-3).
#[test]
fn a_write_that_stops_partway_leaves_the_deck_as_it_was() {
    let dir = scratch("write-partway").join("b1.scaena");
    copy_dir(Path::new("../../tests/bench/b1.scaena"), &dir);
    let bundle = Bundle::open(&dir).unwrap();
    let before = std::fs::read(dir.join("deck.json")).unwrap();
    std::fs::create_dir(dir.join("notes.txt")).unwrap();
    let files = [("deck.json".to_string(), b"{}".to_vec()), ("notes.txt".to_string(), b"mine".to_vec())].into();
    let err = bundle.write(&files).unwrap_err();
    assert!(matches!(err, scaena_store::StoreError::Io(_)), "{err}");
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before, "deck.json comes last");
    std::fs::remove_dir(dir.join("notes.txt")).unwrap();
    bundle.write(&files).unwrap();
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), b"{}");
    assert_eq!(std::fs::read(dir.join("notes.txt")).unwrap(), b"mine");
}

/// A folder bundle taken from git or an archive keeps its links. One that leads out of the
/// bundle is never followed: a font linked to a file outside is not read into a save, a data
/// folder linked outside is not written into, and a link to `/` is not walked (SPEC §3.1).
#[cfg(unix)]
#[test]
fn a_link_out_of_the_bundle_is_neither_read_written_nor_walked() {
    use std::os::unix::fs::symlink;
    let root = scratch("links");
    let dir = root.join("b1.scaena");
    copy_dir(Path::new("../../tests/bench/b1.scaena"), &dir);
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    // A font the deck names, moved outside and linked back in: the same bytes, a link out.
    let font = std::fs::read_dir(dir.join("fonts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "ttf" || e == "otf"))
        .unwrap();
    let name = font.file_name().unwrap().to_owned();
    std::fs::rename(&font, outside.join(&name)).unwrap();
    symlink(outside.join(&name), &font).unwrap();
    // A data folder linked outside, and a link to the root of the disk.
    symlink(&outside, dir.join("data")).unwrap();
    symlink("/", dir.join("everything")).unwrap();

    // Validation says why the deck's font is not there.
    let (deck, unparsed) = scaena_store::open_unparsed(&dir).unwrap();
    let found = validate_bundle(&deck, &unparsed).unwrap();
    assert!(
        found.iter().any(|f| f.code == "E102" && f.message.contains("a link that leads out of the bundle")),
        "{found:#?}"
    );
    let bundle = Bundle::open(&dir).unwrap();
    let rel = format!("fonts/{}", name.to_string_lossy());
    let err = bundle.read(&rel).unwrap_err().to_string();
    assert!(err.contains("leads out of the bundle"), "{err}");
    // Listed, it is neither the linked font nor anything under the links to folders.
    let listed = bundle.files.list().unwrap();
    assert!(!listed.contains(&rel), "{listed:?}");
    assert!(listed.iter().all(|f| !f.starts_with("data/") && !f.starts_with("everything/")), "{listed:?}");
    // A save refuses the font rather than carrying its bytes.
    let saved = bundle.save(&root.join("saved.scaena"), &opts());
    assert!(saved.is_err_and(|e| e.to_string().contains("leads out of the bundle")));
    // A data edit through the linked folder writes nothing outside.
    let mut files = std::collections::BTreeMap::new();
    files.insert("data/q3.csv".to_string(), b"region,revenue\nGulf,4.1\n".to_vec());
    let written = bundle.write(&files).unwrap_err().to_string();
    assert!(written.contains("leads out of the bundle"), "{written}");
    assert!(!outside.join("q3.csv").exists());
}
