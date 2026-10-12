//! Rich Text as plain text: its words, a paragraph a line, the font and color tables, the
//! stylesheet, its pictures, and any destination a reader may skip left out.

/// Windows-1252's characters for its bytes 0x80–0x9F; the rest are Latin-1's.
const CP1252: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}', '\u{2030}',
    '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}',
    '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}',
    '\u{17e}', '\u{178}',
];

fn cp1252(b: u8) -> char {
    match b {
        0x80..=0x9f => CP1252[(b - 0x80) as usize],
        b => b as char,
    }
}

/// Destinations whose words are not the document's.
const SKIPPED: [&str; 12] = [
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "header",
    "footer",
    "listtable",
    "listoverridetable",
    "themedata",
    "datastore",
    "xmlnstbl",
];

/// `text`, RTF, as plain text.
pub fn read(text: &str) -> String {
    let s = text.as_bytes();
    let mut out = String::new();
    // Per group: whether its words are skipped, and how many characters a \u stands in for.
    let mut stack: Vec<(bool, usize)> = vec![(false, 1)];
    let mut skip_after_unicode = 0usize;
    let mut i = 0;
    while i < s.len() {
        let (skipping, uc) = *stack.last().expect("the document's group");
        match s[i] {
            b'{' => {
                stack.push((skipping, uc));
                i += 1;
            }
            b'}' => {
                if stack.len() > 1 {
                    stack.pop();
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&c) = s.get(i) else { break };
                if c == b'\'' {
                    // A byte in the code page, in hex.
                    let hex = std::str::from_utf8(s.get(i + 1..i + 3).unwrap_or_default()).unwrap_or_default();
                    i += 3;
                    if skip_after_unicode > 0 {
                        skip_after_unicode -= 1;
                    } else if let (false, Ok(b)) = (skipping, u8::from_str_radix(hex, 16)) {
                        out.push(cp1252(b));
                    }
                    continue;
                }
                if !c.is_ascii_alphabetic() {
                    // A control symbol: an escaped brace or backslash, a nonbreaking space, a skip.
                    i += 1;
                    match c {
                        b'*' => {
                            if let Some(top) = stack.last_mut() {
                                top.0 = true;
                            }
                        }
                        b'{' | b'}' | b'\\' if !skipping => out.push(c as char),
                        b'~' if !skipping => out.push('\u{a0}'),
                        b'\n' | b'\r' if !skipping => out.push('\n'),
                        _ => {}
                    }
                    continue;
                }
                let start = i;
                while i < s.len() && s[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = std::str::from_utf8(&s[start..i]).unwrap_or_default();
                let num_start = i;
                if i < s.len() && (s[i] == b'-' || s[i].is_ascii_digit()) {
                    i += 1;
                    while i < s.len() && s[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let param: Option<i32> = std::str::from_utf8(&s[num_start..i]).ok().and_then(|n| n.parse().ok());
                if i < s.len() && s[i] == b' ' {
                    i += 1;
                }
                if SKIPPED.contains(&word) {
                    if let Some(top) = stack.last_mut() {
                        top.0 = true;
                    }
                    continue;
                }
                if skipping {
                    continue;
                }
                match word {
                    "par" | "line" | "row" | "sect" | "page" => out.push('\n'),
                    "tab" | "cell" => out.push('\t'),
                    "emdash" => out.push('\u{2014}'),
                    "endash" => out.push('\u{2013}'),
                    "lquote" => out.push('\u{2018}'),
                    "rquote" => out.push('\u{2019}'),
                    "ldblquote" => out.push('\u{201c}'),
                    "rdblquote" => out.push('\u{201d}'),
                    "bullet" => out.push('\u{2022}'),
                    "uc" => {
                        if let Some(top) = stack.last_mut() {
                            top.1 = param.unwrap_or(1).max(0) as usize;
                        }
                    }
                    "u" => {
                        let code = param.unwrap_or(0);
                        let code = if code < 0 { code + 65536 } else { code } as u32;
                        if let Some(ch) = char::from_u32(code) {
                            out.push(ch);
                        }
                        skip_after_unicode = uc;
                    }
                    _ => {}
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                // A character as written: ASCII, as RTF writes it, or UTF-8 where a file has it raw.
                let c = text.get(i..).and_then(|rest| rest.chars().next());
                i += c.map_or(1, char::len_utf8);
                if skip_after_unicode > 0 {
                    skip_after_unicode -= 1;
                } else if let (false, Some(c)) = (skipping, c) {
                    out.push(c);
                }
            }
        }
    }
    let lines: Vec<&str> = out.lines().map(str::trim_end).collect();
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_text_reads_as_its_words() {
        let rtf = r"{\rtf1\ansi\deff0{\fonttbl{\f0 Helvetica;}}{\colortbl;\red0\green0\blue0;}
{\*\generator Riched20;}\f0\fs24 Q3 revenue grew \b 41%\b0 .\par
Caf\'e9 \u8212? done\par}";
        assert_eq!(read(rtf), "Q3 revenue grew 41%.\nCaf\u{e9} \u{2014} done");
        // Characters written raw, as some writers leave them, read as themselves.
        assert_eq!(read("{\\rtf1 Z\u{fc}rich \u{2192} Gen\u{e8}ve}"), "Z\u{fc}rich \u{2192} Gen\u{e8}ve");
    }
}
