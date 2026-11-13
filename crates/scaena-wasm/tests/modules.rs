//! What the editor's module leaves to the modules a page loads beside it, held by the features its
//! build resolves: `cargo tree`, run as the module is built, for wasm32.

use std::process::Command;

/// The features `cargo tree` resolves on `package`'s build for the browser, as it prints them.
fn features(package: &str) -> String {
    let out = Command::new(env!("CARGO"))
        .args(["tree", "-p", package, "--target", "wasm32-unknown-unknown", "-e", "features", "-i", "scaena-sources"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("cargo prints UTF-8")
}

/// The editor's module reads no PDF (W1's dogfood 17): `scaena-sources`' `pdf` feature, the reader
/// of the words and figures a PDF draws, is the generator's module's alone, which a page loads to
/// make a deck from a prompt. The editor's carries `scaena-sources` to make a bundle of the deck the
/// generator built, never its PDF reader.
#[test]
fn the_editors_module_reads_no_pdf() {
    let editor = features("scaena-wasm");
    assert!(editor.contains("scaena-sources"), "the editor's module carries the sources' crate: {editor}");
    assert!(!editor.contains(r#"scaena-sources feature "pdf""#), "and not its PDF reader: {editor}");
    let generator = features("scaena-generator");
    assert!(generator.contains(r#"scaena-sources feature "pdf""#), "the generator's module reads PDFs: {generator}");
}
