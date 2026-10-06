//! The crates the browser's modules carry sort stably through `scaena_core::sort`, which
//! compiles one merge sort for all of them. `slice::sort_by` and its kin compile a whole sort
//! for each element type and comparator they are called with: some 10 KB of WASM a call site,
//! which took the editor's module past SPEC §15's 3 MB. Tests may sort as they like; an
//! unstable sort of numbers is shared already, and stays.

use std::path::Path;

/// The crates the editor's module, the player's, and the history's are built from.
const CARRIED: [&str; 5] = ["scaena-core", "scaena-engine", "scaena-ops", "scaena-store", "scaena-wasm"];

/// The stable sorts of `slice` and `Vec`, as a call reads.
const SORTS: [&str; 4] = [".sort()", ".sort_by(", ".sort_by_key(", ".sort_by_cached_key("];

#[test]
fn the_crates_the_modules_carry_sort_through_one_compiled_sort() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    for krate in CARRIED {
        let mut files = Vec::new();
        walk(&root.join("crates").join(krate).join("src"), &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            // A file's tests come last, in `mod tests`, which other tests may share.
            let tests = ["#[cfg(test)]\nmod tests", "#[cfg(test)]\npub(crate) mod tests"];
            let code = tests.iter().filter_map(|t| text.find(t)).min().map_or(text.as_str(), |at| &text[..at]);
            for (n, line) in code.lines().enumerate() {
                let line = line.trim_start();
                if !line.starts_with("//") && SORTS.iter().any(|s| line.contains(s)) {
                    found.push(format!("{}:{}: {line}", file.strip_prefix(&root).unwrap().display(), n + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "sort through `scaena_core::sort` (`by`, `by_key`, `sort`):\n{}", found.join("\n"));
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("scaena-core/src/sort.rs") {
            out.push(path);
        }
    }
}
