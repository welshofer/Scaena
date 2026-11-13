//! Gate 3's third criterion (PLAN §Phase 3, SPEC §9.3), and gate 4's (SPEC §9.4): the Mac's and
//! the iPad's SwiftUI own chrome only, and every frame, glyph, and layout of a deck is the
//! engine's. No Swift in `apps/mac` or `apps/ipad` lays text out or measures it with Core Text or
//! TextKit, by AppKit or UIKit, or draws a string itself. The one exception is the source pane: an
//! `NSTextView` of the deck's `.scn`, or the iPad's `UITextView`, which is chrome, as the
//! browser's CodeMirror is (invariant 8 bars TextKit from the render path).

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
    "UITextView",
    "UITextField",
    "UILabel",
    "boundingRect(with",
    "size(withAttributes",
    "draw(at:",
    "draw(in:",
    "draw(with:",
];

/// The chrome allowed TextKit: the source pane, an editor of the deck's `.scn`.
const SOURCE_PANE: &str = "SourcePane.swift";

/// What a text view posts, named by its class: the canvas hears a field elsewhere stop taking the
/// keys (`UITextField.textDidEndEditingNotification`, PLAN 3.28), and hearing it lays no text out.
const HEARD: &[&str] = &["UITextField.textDid", "UITextView.textDid"];

/// The two text classes named as class objects, in DEBUG builds alone, so that the iPad's UI tests'
/// hardware pass answers their input view as nothing and iOS brings up no keyboard of its own
/// (`Keyboardless.swift`, #1240). That names the classes to swap one getter's implementation: it
/// makes no field, and lays out, measures, and draws no text, so the scan reads past it. Any other
/// use of either class, a field made or a text view's layout, is still found.
const CLASSES: &str = "[UITextField.self, UITextView.self] as [AnyClass]";

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
fn the_apps_swift_lays_no_text_out() {
    let mut files = Vec::new();
    swift(Path::new("../../apps/mac/ScaenaKit/Sources"), &mut files);
    swift(Path::new("../../apps/ipad"), &mut files);
    assert!(files.len() > 10, "the app's Swift is where it was: {files:?}");
    let mut found = Vec::new();
    for file in &files {
        let pane = file.file_name().is_some_and(|n| n == SOURCE_PANE);
        let text = std::fs::read_to_string(file).unwrap();
        for (line, what) in laid_out(&text, pane) {
            found.push(format!("{}:{line}: {what}", file.display()));
        }
    }
    assert!(found.is_empty(), "text laid out, measured, or drawn outside the engine:\n{}", found.join("\n"));
}

/// Each line of `text`, a Swift file, that lays text out, measures it, or draws it, from 1, with
/// what it names; `pane` where the file is the source pane, which may hold a text view.
fn laid_out(text: &str, pane: bool) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        let code = HEARD.iter().fold(code.replace(CLASSES, ""), |code, heard| code.replace(heard, ""));
        for what in LAYOUT {
            if code.contains(what) && !(pane && (*what == "NSTextView" || *what == "UITextView")) {
                found.push((i + 1, *what));
            }
        }
    }
    found
}

#[test]
fn the_classes_named_for_their_input_view_lay_nothing_out_and_nothing_else_passes() {
    let named = "        for kind in [UITextField.self, UITextView.self] as [AnyClass] {\n";
    assert!(laid_out(named, false).is_empty(), "the classes named for their input view pass");
    let made = format!("{named}        let field = UITextField(frame: .zero)\n        let view = UITextView()\n");
    assert_eq!(laid_out(&made, false), [(2, "UITextField"), (3, "UITextView")], "a field or a text view made is found");
    // The phrase alone passes: a class named otherwise on its line is still found.
    assert_eq!(laid_out("[UITextField.self] as [AnyClass]\n", false), [(1, "UITextField")]);
}
