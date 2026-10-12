//! What Narrato's TypeScript means by its string and number primitives, so the port reads the
//! same input the same way: JavaScript's `\s` and `trim`, a `slice` counted in UTF-16 units, and
//! `Math.round`.

/// Whether `c` is whitespace as JavaScript's `\s` and `trim` read it: the Unicode spaces and
/// line terminators, and the byte-order mark, but not U+0085.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// `text.trim()`.
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// `text.slice(0, n)`: the first `n` UTF-16 units, short of a pair it would split.
pub fn first_units(text: &str, n: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for c in text.chars() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        out.push(c);
    }
    out
}

/// `text.replace(/\s+/g, " ").trim()`.
pub fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if is_space(c) {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// `Math.round(x)`: halves toward +∞.
pub fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_numbers_read_as_javascript_reads_them() {
        assert_eq!(trim("\u{FEFF} a b \u{3000}"), "a b");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
        assert_eq!(first_units("a😀b", 2), "a");
        assert_eq!(first_units("a😀b", 3), "a😀");
        assert_eq!(collapse_spaces("  ###\n\t Instruction  "), "### Instruction");
        assert_eq!((round(2.5), round(-2.5), round(83.49)), (3.0, -2.0, 83.0));
    }
}
