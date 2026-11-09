//! A bundle opened by a bare `deck.json`, from the folder that holds it. Its own test binary,
//! since it sets the process's current directory.

use scaena_store::Bundle;
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() { copy_dir(&path, &target) } else { std::fs::copy(&path, &target).map(|_| ()).unwrap() }
    }
}

/// `scaena save deck.json` run in the bundle's folder keeps the guard a folder's path keeps: a
/// font linked out of the bundle is neither read nor carried into a save (lift-up 20261031
/// SEC-2).
#[cfg(unix)]
#[test]
fn a_bare_deck_json_follows_no_link_out_of_its_folder() {
    use std::os::unix::fs::symlink;
    let bench = std::fs::canonicalize("../../tests/bench/b1.scaena").unwrap();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store-bare");
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("b1.scaena");
    copy_dir(&bench, &dir);
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let font = std::fs::read_dir(dir.join("fonts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "ttf" || e == "otf"))
        .unwrap();
    let name = font.file_name().unwrap().to_owned();
    std::fs::rename(&font, outside.join(&name)).unwrap();
    symlink(outside.join(&name), &font).unwrap();

    std::env::set_current_dir(&dir).unwrap();
    let bundle = Bundle::open(Path::new("deck.json")).unwrap();
    let rel = format!("fonts/{}", name.to_string_lossy());
    let err = bundle.read(&rel).unwrap_err().to_string();
    assert!(err.contains("leads out of the bundle"), "{err}");
    let saved = bundle.save(
        &root.join("saved.scaena"),
        &scaena_store::SaveOptions { subset_fonts: true, now: "2026-10-31T00:00:00Z".into(), history: false },
    );
    assert!(saved.is_err_and(|e| e.to_string().contains("leads out of the bundle")));
}
