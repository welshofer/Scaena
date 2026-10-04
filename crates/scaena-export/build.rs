//! A single-file HTML export carries the web player's page (PLAN 2.5), which `just web`
//! builds into `player/standalone.html`. With it there, the crate builds with `cfg(player)`
//! and carries it; without it, `export --format html` says to build it. Cargo runs this
//! again whenever anything in `player/` changes.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(player)");
    println!("cargo::rerun-if-changed=player");
    if std::path::Path::new("player/standalone.html").is_file() {
        println!("cargo::rustc-cfg=player");
    }
}
