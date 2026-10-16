//! With the `shipped` feature, every font the themes that ship name, compressed into the binary
//! (PLAN T.0c, ADR-0031 decision 5). `src/shipped/fonts.tsv`, which `scripts/build_themes.py`
//! writes from the registry, says where each is: a file in the repository (the four themes'), or
//! an upstream file by its sha256 in the cache `scripts/build_bundle_fonts.py --fetch --shipped`
//! fills (`$SCAENA_FONT_CACHE`, else `~/.cache/scaena-fonts`). Nothing here reaches the network:
//! a font the cache lacks, or one whose bytes are not its pin's, fails the build, naming its family.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_SHIPPED").is_none() {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let listed = manifest.join("src/shipped/fonts.tsv");
    println!("cargo:rerun-if-changed={}", listed.display());
    println!("cargo:rerun-if-env-changed=SCAENA_FONT_CACHE");
    let cache = cache();
    let text = std::fs::read_to_string(&listed).unwrap_or_else(|e| fail(&format!("{}: {e}", listed.display())));
    let mut rows = String::new();
    let mut n = 0;
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let [path, family, source] = line.split('\t').collect::<Vec<_>>()[..] else {
            fail(&format!("{}: `{line}` is not a path, a family, and a source", listed.display()))
        };
        let bytes = match source.strip_prefix("sha256:") {
            Some(sha) => cached(&cache, path, family, sha),
            None => {
                let file = root.join(source);
                println!("cargo:rerun-if-changed={}", file.display());
                std::fs::read(&file)
                    .unwrap_or_else(|e| fail(&format!("{path}, of the family `{family}`: {source}: {e}")))
            }
        };
        let packed = format!("font-{n}.deflate");
        let mut deflate = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        deflate.write_all(&bytes).unwrap();
        std::fs::write(out.join(&packed), deflate.finish().unwrap()).unwrap();
        writeln!(
            rows,
            "    Font {{ path: {path:?}, family: {family:?}, size: {}, packed: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{packed}\")) }},",
            bytes.len()
        )
        .unwrap();
        n += 1;
    }
    let table = format!(
        "/// Every font the themes that ship name, compressed, as `src/shipped/fonts.tsv` lists them.\n\
         const FONTS: [Font; {n}] = [\n{rows}];\n"
    );
    std::fs::write(out.join("fonts.rs"), table).unwrap();
}

/// The cache of upstream files by sha256 (PLAN T.0b).
fn cache() -> PathBuf {
    if let Some(dir) = std::env::var_os("SCAENA_FONT_CACHE") {
        return dir.into();
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default();
    Path::new(&home).join(".cache/scaena-fonts")
}

/// The upstream file pinned as `sha`, from the cache, held to its pin.
fn cached(cache: &Path, path: &str, family: &str, sha: &str) -> Vec<u8> {
    use sha2::Digest as _;
    let file = cache.join(sha);
    println!("cargo:rerun-if-changed={}", file.display());
    let Ok(bytes) = std::fs::read(&file) else {
        fail(&format!(
            "{path}, of the family `{family}`, is not in the font cache at {}: run \
             `python3 scripts/build_bundle_fonts.py --fetch --shipped`",
            cache.display()
        ))
    };
    let digest = sha2::Sha256::digest(&bytes);
    let got = digest.iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    });
    if got != sha {
        fail(&format!("{path}, of the family `{family}`: {} is {got}, not its pin {sha}", file.display()));
    }
    bytes
}

fn fail(said: &str) -> ! {
    eprintln!("error: the fonts the themes that ship name (PLAN T.0c): {said}");
    std::process::exit(1)
}
