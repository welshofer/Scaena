//! Every build script in the workspace reads its crate's folder as Cargo runs it, never as it was
//! compiled. One target folder shared by several checkouts (git worktrees) compiles a build script
//! once for all of them, since Cargo hashes a workspace crate by its path from the workspace's
//! root. A path compiled in with `env!` names the first checkout, so once that one is removed,
//! every other checkout's build panics on a file that is gone.

use std::path::Path;

#[test]
fn no_build_script_compiles_in_its_crates_folder() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut read = 0;
    for entry in std::fs::read_dir(&crates).expect("the workspace's crates") {
        let script = entry.expect("a crate").path().join("build.rs");
        let Ok(text) = std::fs::read_to_string(&script) else { continue };
        read += 1;
        for (i, line) in text.lines().enumerate() {
            assert!(
                !line.contains("env!(\"CARGO_MANIFEST_DIR\")"),
                "{}:{}: a build script reads CARGO_MANIFEST_DIR with std::env::var as Cargo runs it, not env! as it compiles it",
                script.display(),
                i + 1
            );
        }
    }
    assert!(read >= 4, "the workspace's build scripts were read: {read}");
}
