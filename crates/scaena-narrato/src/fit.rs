//! Words that run past their place (ADR-0026, PLAN G.2): each text a generated deck sets that
//! does not fit its slot, even shrunk to its role's smallest size, rewritten by the model to the
//! length the engine measured for it. Scaena's own call, not Narrato's: Vzestup draws a slide as
//! a picture, where no words run past a place.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A text that runs past its place: the node it is, its words, and the most characters that
/// would fit, by how far it runs over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Long {
    pub id: String,
    pub text: String,
    pub most: usize,
}

/// How many times the model is asked to rewrite what runs past its place: once, then once more
/// for what its rewrite still left too long (STAB-4).
pub const ROUNDS: usize = 2;

/// The rider the rewrite is asked under.
pub const FIT_SYSTEM: &str = "You are tightening the words on presentation slides so each fits the place it is set in. Output ONLY a JSON object \u{2014} no markdown, no code fence, no commentary \u{2014} with one key, \"texts\": an array with one object for each text given, each with \"id\" (string, as given) and \"text\" (string: its words rewritten).";

/// The prompt that asks for `longs` rewritten, in `language` where the deck is in one.
pub fn prompt(longs: &[Long], language: Option<&str>) -> String {
    let mut out = String::from(
        "Each text below is set on a slide and runs past its place. Rewrite each in at most the number of characters given. Keep its meaning, its figures, and its names; keep a title a title; keep a list a list, one item to a line, as many lines as it has. Drop words, never facts.",
    );
    if let Some(language) = language.filter(|l| !l.is_empty() && *l != "English") {
        out.push_str(&format!(" Write each in {language}, as it is written."));
    }
    out.push_str("\n\n");
    for long in longs {
        out.push_str(&format!("id: {}\nat most: {} characters\ntext:\n{}\n\n", long.id, long.most, long.text));
    }
    out
}

/// The answer's contract, as Gemini takes it (the others derive theirs from it).
pub fn schema_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "texts": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": { "id": { "type": "STRING" }, "text": { "type": "STRING" } },
                    "required": ["id", "text"]
                }
            }
        },
        "required": ["texts"]
    })
}

/// The rewrites an answer gives, each kept where it names a text asked about, says something,
/// is shorter than the text it rewrites and within a tenth of the characters asked for, and
/// writes its figures (REL-2); the first for a text, where an answer gives it twice (STAB-4).
pub fn read(text: &str, longs: &[Long]) -> Result<Vec<(String, String)>, String> {
    let start = text.find('{').ok_or("the answer holds no JSON object")?;
    let end = text.rfind('}').ok_or("the answer holds no JSON object")?;
    let value: Value =
        serde_json::from_str(&text[start..=end]).map_err(|e| format!("the answer is not the JSON asked for: {e}"))?;
    let texts = value.get("texts").and_then(Value::as_array).ok_or("the answer has no `texts`")?;
    let mut kept: Vec<(String, String)> = Vec::new();
    for (id, text) in texts
        .iter()
        .filter_map(|t| Some((t.get("id")?.as_str()?.to_string(), t.get("text")?.as_str()?.trim().to_string())))
    {
        let n = text.chars().count();
        let fits = longs.iter().any(|l| {
            l.id == id
                && !text.is_empty()
                && n < l.text.chars().count()
                && n <= l.most + l.most / 10
                && crate::same_figures(&l.text, &text)
        });
        if fits && !kept.iter().any(|(was, _)| *was == id) {
            kept.push((id, text));
        }
    }
    Ok(kept)
}

/// What still runs past its place once the rewrites are done, as the deck made says it: how many
/// texts, and the first words of each.
pub fn still(longs: &[Long]) -> String {
    let words: Vec<String> = longs
        .iter()
        .map(|l| {
            let mut first: String = l.text.split_whitespace().take(5).collect::<Vec<_>>().join(" ");
            if first.chars().count() < l.text.trim().chars().count() {
                first.push('\u{2026}');
            }
            format!("\u{201c}{first}\u{201d}")
        })
        .collect();
    match longs.len() {
        1 => format!("1 text still runs past its place: {}", words[0]),
        n => format!("{n} texts still run past their place: {}", words.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long() -> Long {
        Long {
            id: "a-header".into(),
            text: "Freight emissions are still climbing across every corridor we run".into(),
            most: 40,
        }
    }

    #[test]
    fn the_prompt_says_how_long_each_may_be() {
        let p = prompt(&[long()], Some("Czech"));
        assert!(p.contains("id: a-header\nat most: 40 characters\ntext:\nFreight emissions"));
        assert!(p.contains("Write each in Czech"));
        assert!(!prompt(&[long()], Some("English")).contains("Write each in"));
    }

    #[test]
    fn a_rewrite_is_kept_where_it_is_shorter_and_asked_for() {
        let answer = r#"{"texts": [
            {"id": "a-header", "text": "Freight emissions keep climbing"},
            {"id": "not-asked", "text": "x"},
            {"id": "a-header", "text": "Freight emissions are still climbing across every corridor we run, and more"}
        ]}"#;
        let read = read(answer, &[long()]).unwrap();
        assert_eq!(read, vec![("a-header".to_string(), "Freight emissions keep climbing".to_string())]);
        assert!(super::read("no json", &[long()]).is_err());
        // A rewrite that changes a figure is no rewrite of the same words (REL-2).
        let figured =
            Long { id: "f".into(), text: "Emissions per lane fell 41% across 38 pilot shippers".into(), most: 36 };
        let answer = r#"{"texts": [{"id": "f", "text": "Emissions fell about 40% in pilots"}]}"#;
        assert!(super::read(answer, std::slice::from_ref(&figured)).unwrap().is_empty());
        let answer = r#"{"texts": [{"id": "f", "text": "Lane emissions fell 41% at 38 shippers"}]}"#;
        assert_eq!(super::read(answer, &[figured]).unwrap().len(), 1);
    }

    #[test]
    fn a_rewrite_still_too_long_is_not_kept_and_a_second_for_a_text_is_not_either() {
        // 40 characters asked for: 44 is within a tenth, 45 is not (STAB-4).
        let at = |n: usize| format!("{{\"texts\": [{{\"id\": \"a-header\", \"text\": \"{}\"}}]}}", "x".repeat(n));
        assert_eq!(read(&at(44), &[long()]).unwrap().len(), 1);
        assert!(read(&at(45), &[long()]).unwrap().is_empty());
        let twice = r#"{"texts": [
            {"id": "a-header", "text": "Freight emissions keep climbing"},
            {"id": "a-header", "text": "Emissions keep climbing"}
        ]}"#;
        assert_eq!(
            read(twice, &[long()]).unwrap(),
            vec![("a-header".into(), "Freight emissions keep climbing".into())]
        );
    }

    #[test]
    fn what_still_runs_over_is_said_by_its_first_words() {
        assert_eq!(
            still(&[long()]),
            "1 text still runs past its place: \u{201c}Freight emissions are still climbing\u{2026}\u{201d}"
        );
        let short = Long { id: "b".into(), text: "Net zero".into(), most: 8 };
        assert_eq!(
            still(&[long(), short]),
            "2 texts still run past their place: \u{201c}Freight emissions are still climbing\u{2026}\u{201d}, \u{201c}Net zero\u{201d}"
        );
    }
}
