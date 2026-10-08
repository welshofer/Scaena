//! The crates the browser's modules carry sort stably through `scaena_core::sort`, which
//! compiles one merge sort for all of them. `slice::sort_by` and its kin compile a whole sort
//! for each element type and comparator they are called with: some 10 KB of WASM a call site,
//! which took the editor's module past SPEC §15's 3 MB. `collect` into a `BTreeMap` or a
//! `BTreeSet` sorts first, and compiles that sort for each type and caller, so those crates
//! build one through `sort::map`, `sort::try_map`, or `sort::set` (PLAN 2.91). Tests may sort
//! as they like; an unstable sort of numbers is shared already, and stays.

use std::path::Path;

/// The crates the editor's module, the player's, and the history's are built from. Core sits
/// on `scaena-pixels`, which cannot reach `scaena_core::sort`, and sorts nothing.
const CARRIED: [&str; 7] =
    ["scaena-core", "scaena-engine", "scaena-ops", "scaena-pixels", "scaena-session", "scaena-store", "scaena-wasm"];

/// The stable sorts of `slice` and `Vec`, as a call reads.
const SORTS: [&str; 4] = [".sort()", ".sort_by(", ".sort_by_key(", ".sort_by_cached_key("];

#[test]
fn the_crates_the_modules_carry_sort_through_one_compiled_sort() {
    let mut found = Vec::new();
    for (file, code) in carried() {
        for (n, line) in code.lines().enumerate() {
            let line = line.trim_start();
            if !line.starts_with("//") && SORTS.iter().any(|s| line.contains(s)) {
                found.push(format!("{file}:{}: {line}", n + 1));
            }
        }
    }
    assert!(found.is_empty(), "sort through `scaena_core::sort` (`by`, `by_key`, `sort`):\n{}", found.join("\n"));
}

#[test]
fn the_crates_the_modules_carry_build_ordered_maps_without_a_sort() {
    let mut found = Vec::new();
    for (file, code) in carried() {
        // Statement by statement, each from the line the `;` before it ends.
        let mut line = 1;
        for statement in code.split_inclusive(';') {
            let lines: Vec<&str> = statement.lines().collect();
            // A `let` typed as an ordered map or set whose value is collected.
            if let Some(start) = lines.iter().rposition(|l| l.trim_start().starts_with("let ")) {
                let text = lines[start..].join("\n");
                let typed = text.split_once('=').is_some_and(|(left, _)| ordered(left));
                if typed && text.trim_end().trim_end_matches(';').trim_end().ends_with(".collect()") {
                    found.push(format!("{file}:{}: {}", line + start, lines[start].trim()));
                }
            }
            // A `collect` told to make one.
            for (i, _) in statement.match_indices("collect::<") {
                let turbofish = &statement[i..statement[i..].find("()").map_or(statement.len(), |e| i + e)];
                if ordered(turbofish) {
                    let at = statement[..i].matches('\n').count();
                    found.push(format!("{file}:{}: {}", line + at, lines.get(at).copied().unwrap_or_default().trim()));
                }
            }
            line += statement.matches('\n').count();
        }
    }
    let hint = "build it through `scaena_core::sort` (`map`, `try_map`, `set`), which inserts each item";
    assert!(
        found.is_empty(),
        "a sorted map collected, which compiles a sort for its type and caller: {hint}:\n{}",
        found.join("\n")
    );
}

/// Whether a type names a map or a set that keeps its keys in order.
fn ordered(ty: &str) -> bool {
    ty.contains("BTreeMap<") || ty.contains("BTreeSet<") || ty.contains("BTreeMap>") || ty.contains("BTreeSet>")
}

/// Each source file of the crates the modules carry, by its path from the repository's root,
/// with its code before its tests: a file's tests come last, in `mod tests`, which other tests
/// may share.
fn carried() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    for krate in CARRIED {
        let mut files = Vec::new();
        walk(&root.join("crates").join(krate).join("src"), &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let tests = ["#[cfg(test)]\nmod tests", "#[cfg(test)]\npub(crate) mod tests"];
            let code = tests.iter().filter_map(|t| text.find(t)).min().map_or(text.as_str(), |at| &text[..at]);
            out.push((file.strip_prefix(&root).unwrap().display().to_string(), code.to_string()));
        }
    }
    out
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
