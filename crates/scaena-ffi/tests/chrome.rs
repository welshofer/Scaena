//! Gate 3's third criterion (PLAN §Phase 3, SPEC §9.3): the Mac's SwiftUI owns chrome only, and
//! every frame, glyph, and layout of a deck is the engine's. No Swift in `apps/mac` lays text out
//! or measures it with Core Text or TextKit, or draws a string itself. The one exception is the
//! source pane: an `NSTextView` of the deck's `.scn`, which is chrome, as the browser's CodeMirror
//! is (invariant 8 bars TextKit from the render path).

use std::path::{Path, PathBuf};

/// What lays text out, measures it, or draws it outside the engine.
const LAYOUT: &[&str] = &[
    "import CoreText",
    "CTFramesetter",
    "CTTypesetter",
    "CTLine",
    "CTRun",
    "CTFont",
    "NSLayoutManager",
    "NSTextLayoutManager",
    "NSTextContainer",
    "NSTextStorage",
    "NSTypesetter",
    "NSTextView",
    "NSTextField",
    "boundingRect(with",
    "size(withAttributes",
    "draw(at:",
    "draw(in:",
    "draw(with:",
];

/// The chrome allowed TextKit: the source pane, an editor of the deck's `.scn`.
const SOURCE_PANE: &str = "SourcePane.swift";

fn swift(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            swift(&path, out);
        } else if path.extension().is_some_and(|e| e == "swift") {
            out.push(path);
        }
    }
}

#[test]
fn the_macs_swift_lays_no_text_out() {
    let mut files = Vec::new();
    swift(Path::new("../../apps/mac/ScaenaKit/Sources"), &mut files);
    assert!(files.len() > 10, "the app's Swift is where it was: {files:?}");
    let mut found = Vec::new();
    for file in &files {
        let pane = file.file_name().is_some_and(|n| n == SOURCE_PANE);
        let text = std::fs::read_to_string(file).unwrap();
        for (i, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for what in LAYOUT {
                if code.contains(what) && !(pane && *what == "NSTextView") {
                    found.push(format!("{}:{}: {what}", file.display(), i + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "text laid out, measured, or drawn outside the engine:\n{}", found.join("\n"));
}
