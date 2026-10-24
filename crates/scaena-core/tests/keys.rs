//! The keys a code text answers as a code editor does (PLAN 2.139, ADR-0029): Tab, Shift+Tab, and
//! Return, each from a text and its selection to the text and selection after it.
//!
//! A case writes its text with `|` where the caret stands, or `[` and `]` around the selection.

use scaena_core::code::{Key, Keyed, keyed};

/// `marked`'s text, and its selection by its marks.
fn read(marked: &str) -> (String, usize, usize) {
    let caret = marked.find('|');
    let text: String = marked.chars().filter(|c| !matches!(c, '|' | '[' | ']')).collect();
    match caret {
        Some(at) => (text, at, at),
        None => {
            let from = marked.find('[').unwrap();
            let to = marked.find(']').unwrap() - 1;
            (text, from, to)
        }
    }
}

/// The text and selection, marked as cases write them.
fn marked(k: &Keyed) -> String {
    if k.from == k.to {
        format!("{}|{}", &k.text[..k.from], &k.text[k.from..])
    } else {
        format!("{}[{}]{}", &k.text[..k.from], &k.text[k.from..k.to], &k.text[k.to..])
    }
}

fn press(before: &str, key: Key) -> String {
    press_in(before, key, None)
}

fn press_in(before: &str, key: Key, language: Option<&str>) -> String {
    let (text, from, to) = read(before);
    marked(&keyed(&text, from, to, key, 4, language).unwrap())
}

#[test]
fn tab_stands_to_the_next_stop_or_indents_the_lines() {
    assert_eq!(press("|x", Key::Tab), "    |x");
    assert_eq!(press("ab|c", Key::Tab), "ab  |c");
    assert_eq!(press("abcd|", Key::Tab), "abcd    |");
    // A tab in the text stands to its stop: the column after it is 4.
    assert_eq!(press("a\t|x", Key::Tab), "a\t    |x");
    // A line indented with a tab takes a tab.
    assert_eq!(press("\t|x", Key::Tab), "\t\t|x");
    // A selection on one line is replaced.
    assert_eq!(press("a[bc]d", Key::Tab), "a   |d");
    // Over lines, each is indented a stop, but a line of white space alone.
    assert_eq!(press("[a\n\n  b]", Key::Tab), "    [a\n\n      b]");
    assert_eq!(press("a[b\nc]d\ne", Key::Tab), "    a[b\n    c]d\ne");
    // A selection that ends at a line's start leaves that line.
    assert_eq!(press("[a\n]b", Key::Tab), "    [a\n]b");
}

#[test]
fn shift_tab_takes_a_stop_off_each_line() {
    assert_eq!(press("        |x", Key::ShiftTab), "    |x");
    assert_eq!(press("  x|y", Key::ShiftTab), "x|y");
    assert_eq!(press("\t\tx|", Key::ShiftTab), "\tx|");
    // A caret inside what is taken goes to the line's start.
    assert_eq!(press("  |  x", Key::ShiftTab), "|x");
    assert_eq!(press("x|", Key::ShiftTab), "x|");
    assert_eq!(press("[    a\n  b\nc]", Key::ShiftTab), "[a\nb\nc]");
    assert_eq!(press("    a[\n    b]", Key::ShiftTab), "a[\nb]");
}

#[test]
fn return_indents_as_the_line_and_leaves_no_line_of_indentation_alone() {
    assert_eq!(press("    let x = 1;|", Key::Return), "    let x = 1;\n    |");
    // After an opening bracket, a stop more; between brackets, the closing one on its own line.
    assert_eq!(press("fn main() {|", Key::Return), "fn main() {\n    |");
    assert_eq!(press("fn main() {|}", Key::Return), "fn main() {\n    |\n}");
    assert_eq!(press("    f(|)", Key::Return), "    f(\n        |\n    )");
    // The white space before the caret is taken off the line it leaves.
    assert_eq!(press("    x = 1;   |", Key::Return), "    x = 1;\n    |");
    assert_eq!(press("    |", Key::Return), "\n    |");
    // The rest of the line goes on the new one, at its indentation.
    assert_eq!(press("    a,|  b", Key::Return), "    a,\n    |b");
    // A selection is replaced.
    assert_eq!(press("  a[bc]d", Key::Return), "  a\n  |d");
    // Return twice on an indented line leaves no line of white space behind it.
    let (text, at, _) = read("    x|");
    let once = keyed(&text, at, at, Key::Return, 4, None).unwrap();
    let twice = keyed(&once.text, once.from, once.to, Key::Return, 4, None).unwrap();
    assert_eq!(marked(&twice), "    x\n\n    |");
    // The lines it leaves: none is white space alone.
    let lines: Vec<&str> = twice.text.split('\n').collect();
    assert!(lines[..lines.len() - 1].iter().all(|l| l.is_empty() || !l.trim().is_empty()));
}

/// Code indented with tabs keeps them, as Go's gofmt and `make` take it: a stop added is a tab
/// where the line, else the text, indents with tabs, and always in a Makefile.
#[test]
fn a_stop_is_a_tab_where_the_text_indents_with_tabs() {
    let go = Some("go");
    // Tab, by the line's own indentation, else the first indented line's.
    assert_eq!(press_in("func f() {\n\tx := 1|\n}", Key::Tab, go), "func f() {\n\tx := 1\t|\n}");
    assert_eq!(press_in("func f() {\n\treturn\n}\n|x", Key::Tab, go), "func f() {\n\treturn\n}\n\t|x");
    assert_eq!(press_in("func f() {\n[\ta\n\tb]\n}", Key::Tab, go), "func f() {\n\t[\ta\n\t\tb]\n}");
    // Shift+Tab takes one tab.
    assert_eq!(press_in("func f() {\n[\t\ta\n\t\tb]\n}", Key::ShiftTab, go), "func f() {\n[\ta\n\tb]\n}");
    // Return inside braces: the stop a tab, the closer at the line's own tabs.
    assert_eq!(press_in("func f() {\n\tif ok {|}\n}", Key::Return, go), "func f() {\n\tif ok {\n\t\t|\n\t}\n}");
    // Spaces where the text indents with spaces, whatever its language.
    assert_eq!(press_in("fn f() {\n    x|\n}", Key::Tab, Some("rust")), "fn f() {\n    x   |\n}");

    // A Makefile's recipe is a tab, even before any line is indented, and by any of its names.
    assert_eq!(press_in("all: build\n|", Key::Tab, Some("makefile")), "all: build\n\t|");
    assert_eq!(press_in("all: build\n|", Key::Tab, Some("make")), "all: build\n\t|");
    assert_eq!(press_in("all: build\n|", Key::Tab, None), "all: build\n    |");
    assert_eq!(press_in("all:\n[\techo a\n\techo b]", Key::ShiftTab, Some("makefile")), "all:\n[echo a\necho b]");
    assert_eq!(press_in("all:\n\techo $(|)", Key::Return, Some("makefile")), "all:\n\techo $(\n\t\t|\n\t)");
    // Every indentation each key leaves in them is tabs.
    for (before, key, language) in [
        ("func f() {\n\tif ok {|}\n}", Key::Return, go),
        ("all: build\n|", Key::Tab, Some("makefile")),
        ("all:\n\techo $(|)", Key::Return, Some("makefile")),
    ] {
        let after = press_in(before, key, language).replace('|', "");
        assert!(after.lines().all(|l| !l.starts_with(' ')), "{before:?}: {after:?}");
    }
}

#[test]
fn a_key_is_named_and_a_selection_off_the_text_is_none() {
    assert_eq!(Key::named("Tab"), Some(Key::Tab));
    assert_eq!(Key::named("Shift+Tab"), Some(Key::ShiftTab));
    assert_eq!(Key::named("Enter"), Some(Key::Return));
    assert_eq!(Key::named("Return"), Some(Key::Return));
    assert_eq!(Key::named("Space"), None);
    assert!(keyed("ab", 0, 3, Key::Tab, 4, None).is_none());
    assert!(keyed("é", 1, 1, Key::Tab, 4, None).is_none());
    // A selection given end first reads start first.
    assert_eq!(keyed("ab", 2, 0, Key::Tab, 4, None).unwrap().text, "    ");
}

/// Any text and selection: the keys give a text, a selection on its characters' boundaries, and
/// keep every character but the white space they take and what the selection held.
#[test]
fn any_text_keys_into_a_text() {
    let pieces = ["a", " ", "\t", "\n", "{", "}", "(", ")", "é", "  x", "\n    ", "ü\n"];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for _ in 0..2000 {
        let text: String = (0..next(12)).map(|_| pieces[next(pieces.len())]).collect();
        let bounds: Vec<usize> = (0..=text.len()).filter(|&i| text.is_char_boundary(i)).collect();
        let (a, b) = (bounds[next(bounds.len())], bounds[next(bounds.len())]);
        for key in [Key::Tab, Key::ShiftTab, Key::Return] {
            let k = keyed(&text, a, b, key, 1 + next(8), None).unwrap();
            assert!(k.from <= k.to && k.to <= k.text.len(), "{key:?} on {text:?} {a}..{b}: {k:?}");
            assert!(k.text.is_char_boundary(k.from) && k.text.is_char_boundary(k.to), "{key:?} on {text:?}: {k:?}");
            let words = |t: &str| t.chars().filter(|c| !c.is_whitespace()).count();
            let (lo, hi) = (a.min(b), a.max(b));
            let selected = if key == Key::ShiftTab || (key == Key::Tab && text[lo..hi].contains('\n')) {
                0
            } else {
                words(&text[lo..hi])
            };
            assert_eq!(words(&k.text), words(&text) - selected, "{key:?} on {text:?} {a}..{b}: {k:?}");
        }
    }
}
