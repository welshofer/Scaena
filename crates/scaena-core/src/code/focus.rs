//! A code text's lines in focus as a person reads and writes them (PLAN 2.140, ADR-0029 decision
//! 6): `code.focus` counts lines from 1, as `paragraphs` counts paragraphs, and the gutter numbers
//! them from the text's `code.start`. An editor says a focus, and reads one typed back, in the
//! gutter's numbers: `focus: [[2, 4], 7]` on a text whose gutter starts at 10 is "11–13, 16".

use serde_json::Value;

/// The spans of `focus` as written, each its first and last line counted from 1: a number names
/// one line, a pair a run. What names no line (0), runs backward (E106), or is no span is left
/// out.
pub fn spans(focus: &Value) -> Vec<(u64, u64)> {
    let span = |v: &Value| -> Option<(u64, u64)> {
        let (a, b) = match v {
            Value::Array(pair) if pair.len() == 2 => (pair[0].as_u64()?, pair[1].as_u64()?),
            one => (one.as_u64()?, one.as_u64()?),
        };
        (a >= 1 && a <= b).then_some((a, b))
    };
    focus.as_array().map_or_else(Vec::new, |all| all.iter().filter_map(span).collect())
}

/// `spans` as `code.focus` writes them: a line alone as its number, a run as its first and last.
/// `null` where there are none, which takes the focus away and shows every line.
pub fn value(spans: &[(u64, u64)]) -> Value {
    match spans.is_empty() {
        true => Value::Null,
        false => spans.iter().map(|&(a, b)| if a == b { Value::from(a) } else { Value::from(vec![a, b]) }).collect(),
    }
}

/// `spans` said in the gutter's numbers, the first line numbered `start`: runs with an en dash,
/// apart by commas, "2–4, 7". Empty where there are none.
pub fn said(spans: &[(u64, u64)], start: u64) -> String {
    let number = |line: u64| line + start.max(1) - 1;
    let each = spans.iter().map(|&(a, b)| match a == b {
        true => number(a).to_string(),
        false => format!("{}–{}", number(a), number(b)),
    });
    each.collect::<Vec<_>>().join(", ")
}

/// The spans `words` names in the gutter's numbers, the first of `lines` lines numbered `start`:
/// numbers and runs apart by commas or spaces, a run's ends joined by a hyphen, an en dash, an em
/// dash, or "to" ("2–4, 7", "2-4 7", "2 to 4"). Nothing names no focus. An error says, in a
/// sentence, the first that is not a line the text has or runs backward.
pub fn read(words: &str, start: u64, lines: u64) -> Result<Vec<(u64, u64)>, String> {
    let start = start.max(1);
    let (first, last) = (start, start + lines.max(1) - 1);
    let mut spaced = words.replace(" to ", "-").replace(['–', '—'], "-");
    while spaced.contains(" -") || spaced.contains("- ") {
        spaced = spaced.replace(" -", "-").replace("- ", "-");
    }
    let mut out = Vec::new();
    for item in spaced.split([',', ';', ' ', '\t', '\n']).map(str::trim).filter(|w| !w.is_empty()) {
        let number = |w: &str| -> Result<u64, String> {
            let n: u64 =
                w.trim().parse().map_err(|_| format!("“{item}” is not a line: write a number, or two with a dash"))?;
            match (first..=last).contains(&n) {
                true => Ok(n - start + 1),
                false => Err(format!("The code has no line {n}: its lines are {first} to {last}")),
            }
        };
        let (a, b) = match item.split_once('-') {
            Some((a, b)) => (number(a)?, number(b)?),
            None => (number(item)?, number(item)?),
        };
        if a > b {
            let (a, b) = (a + start - 1, b + start - 1);
            return Err(format!("{a}–{b} runs backward: write {b}–{a}"));
        }
        out.push((a, b));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_focus_reads_and_prints_as_the_gutter_numbers_it() {
        let focus = json!([[2, 4], 7]);
        assert_eq!(spans(&focus), [(2, 4), (7, 7)]);
        assert_eq!(said(&spans(&focus), 1), "2–4, 7");
        // The gutter numbered from 10: line 2 is 11.
        assert_eq!(said(&spans(&focus), 10), "11–13, 16");
        assert_eq!(read("11–13, 16", 10, 8), Ok(vec![(2, 4), (7, 7)]));
        assert_eq!(read("2-4 7", 1, 8), Ok(vec![(2, 4), (7, 7)]));
        assert_eq!(read("2 to 4", 1, 8), Ok(vec![(2, 4)]));
        assert_eq!(read("2 – 4,7", 1, 8), Ok(vec![(2, 4), (7, 7)]));
        assert_eq!(read("  ", 1, 8), Ok(vec![]));
        assert_eq!(value(&read("2–4, 7", 1, 8).unwrap()), focus);
        assert_eq!(value(&[]), Value::Null);
    }

    #[test]
    fn a_focus_that_names_no_line_of_the_text_is_refused_in_a_sentence() {
        assert_eq!(read("9", 1, 8), Err("The code has no line 9: its lines are 1 to 8".into()));
        assert_eq!(read("1", 10, 8), Err("The code has no line 1: its lines are 10 to 17".into()));
        assert_eq!(read("4–2", 1, 8), Err("4–2 runs backward: write 2–4".into()));
        assert!(read("two", 1, 8).unwrap_err().contains("is not a line"));
    }
}
