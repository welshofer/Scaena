//! `scaena serve` carries the web player's and editor's pages (PLAN 2.11, ADR-0012), which
//! `just web` builds into `web/dist` and copies to `pages/dist`. With them there, this gzips
//! each into the build and sets `cfg(pages)`; without them, `scaena serve` says to build them.
//! Cargo runs it again whenever anything in `pages/` changes.

use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(pages)");
    println!("cargo::rerun-if-changed=pages");
    let dist = Path::new("pages/dist");
    if !dist.join("index.html").is_file() || !dist.join("editor.html").is_file() {
        return;
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let mut files = Vec::new();
    walk(dist, "", &mut files);
    files.sort();
    let mut table =
        String::from("/// Each page's file by its path in the build, gzipped.\nstatic PAGES: &[(&str, &[u8])] = &[\n");
    for (i, rel) in files.iter().enumerate() {
        let bytes = std::fs::read(dist.join(rel)).expect("reading a page's file");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&bytes).expect("gzipping a page's file");
        let name = format!("page-{i}.gz");
        std::fs::write(out.join(&name), gz.finish().expect("gzipping a page's file")).expect("writing a page's file");
        table += &format!("    ({rel:?}, include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{name}\"))),\n");
    }
    table += "];\n";
    std::fs::write(out.join("pages.rs"), table).expect("writing the pages' table");
    println!("cargo::rustc-cfg=pages");
}

/// Every file under `dir`, by its path from `pages/dist`.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("reading pages/dist") {
        let entry = entry.expect("reading pages/dist");
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() {
            walk(&entry.path(), &format!("{prefix}{name}/"), out);
        } else {
            out.push(format!("{prefix}{name}"));
        }
    }
}
