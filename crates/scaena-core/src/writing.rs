//! What a model is asked to write for the person editing a deck (PLAN 2.101): a text's words
//! proofread, tightened, or rewritten; a slide's notes drafted from how it reads; a finding
//! explained. Each task is the instructions a model takes and the prompt it answers, on the
//! device (Apple's Foundation Models) or with the person's own key (SPEC §11); the answer is
//! offered, and taken as one patch or left. The browser, the Mac, and the iPad ask in these words,
//! and take an answer by [`replacing`], so a text's runs keep their looks where the words stay.

use serde::{Deserialize, Serialize};

/// A task for a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Task {
    /// A text's spelling, grammar, and punctuation put right, and nothing else changed.
    Proofread { text: String },
    /// A text in fewer words, its meaning kept.
    Tighten { text: String },
    /// A text said more clearly and directly.
    Rewrite { text: String },
    /// Notes for the speaker of a slide: from its words as it reads, a line for each part, and
    /// the point its beat makes, where the spine gives one.
    Notes {
        slide: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        claim: Option<String>,
    },
    /// What a finding of lint means for the slide, and what could be done.
    Explain(Explained),
}

/// A finding to explain: as the window says it, its code and severity, lint's hint, and the
/// words of what it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explained {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
}

impl Task {
    /// What it is called where it is offered.
    pub fn title(&self) -> &'static str {
        match self {
            Task::Proofread { .. } => "Proofread",
            Task::Tighten { .. } => "Tightened",
            Task::Rewrite { .. } => "Rewritten",
            Task::Notes { .. } => "Presenter Notes",
            Task::Explain(_) => "About This Issue",
        }
    }

    /// The text it works on, for a task on a text's words.
    pub fn text(&self) -> Option<&str> {
        match self {
            Task::Proofread { text } | Task::Tighten { text } | Task::Rewrite { text } => Some(text),
            _ => None,
        }
    }

    /// What the model is told it does: its system prompt.
    pub fn instructions(&self) -> &'static str {
        match self {
            Task::Proofread { .. } => {
                "You proofread the words of one text on a slide. Correct its spelling, grammar, and \
                 punctuation, and change nothing else: not its wording, its tone, its capitals where \
                 they are a choice, or its line breaks. Keep every number and name exactly as written. \
                 If nothing needs correcting, write the text exactly as it was given. Write only the \
                 text, with no quotes and no comment."
            }
            Task::Tighten { .. } => {
                "You tighten the words of one text on a slide: fewer words, the same meaning, in the \
                 same language and tone. Keep every number and name exactly as written, and keep one \
                 line for each line you are given. Write only the tightened text, with no quotes and \
                 no comment."
            }
            Task::Rewrite { .. } => {
                "You rewrite the words of one text on a slide so that it reads clearly and directly, \
                 as a skilled presentation writer would: the point first, concrete words, active \
                 verbs. Keep its meaning, its language, and about its length. Keep every number and \
                 name exactly as written, and keep one line for each line you are given. Write only \
                 the rewritten text, with no quotes and no comment."
            }
            Task::Notes { .. } => {
                "You draft speaker notes for one slide of a talk: what the speaker says while it \
                 shows, in two to four short sentences, in the slide's language, from the slide's \
                 words and the point it makes. Write only the notes, with no heading."
            }
            Task::Explain(_) => {
                "You explain a finding of a slide deck's lint to the person writing the deck, in two \
                 or three plain sentences: what it means for their slide, and what they could do \
                 about it. Say nothing the finding does not support, and write no headings or lists."
            }
        }
    }

    /// What the model is asked.
    pub fn prompt(&self) -> String {
        match self {
            Task::Proofread { text } | Task::Tighten { text } | Task::Rewrite { text } => text.clone(),
            Task::Notes { slide, claim } => {
                let mut out = format!("The slide reads:\n{slide}");
                if let Some(claim) = claim.as_deref().filter(|c| !c.trim().is_empty()) {
                    out.push_str(&format!("\nThe point it makes: {claim}"));
                }
                out
            }
            Task::Explain(f) => {
                let mut out = format!("Finding {} ({}): {}", f.code, f.severity, f.message);
                if let Some(hint) = f.hint.as_deref().filter(|h| !h.is_empty()) {
                    out.push_str(&format!("\nLint's hint: {hint}"));
                }
                if let Some(words) = f.words.as_deref().filter(|w| !w.is_empty()) {
                    out.push_str(&format!("\nIts words: {words}"));
                }
                out
            }
        }
    }

    /// The model's answer as it is offered: its ends trimmed, its lines broken as `\n`, and, for
    /// a text's words, the quotes a model may wrap it in taken off where the text had none.
    pub fn taken(&self, answer: &str) -> String {
        let answer = answer.replace("\r\n", "\n").replace('\r', "\n");
        let mut out = answer.trim();
        if let Some(text) = self.text() {
            for (open, close) in [('"', '"'), ('“', '”'), ('\'', '\''), ('‘', '’')] {
                let quoted = out.chars().count() >= 2 && out.starts_with(open) && out.ends_with(close);
                if quoted && !(text.trim_start().starts_with(open) && text.trim_end().ends_with(close)) {
                    out = out[open.len_utf8()..out.len() - close.len_utf8()].trim();
                    break;
                }
            }
        }
        out.to_string()
    }
}

/// The slide's parts as the notes' prompt reads them: a line for each, what it is and its words.
pub fn slide(parts: &[crate::reading::Part]) -> String {
    let lines: Vec<String> = (parts.iter())
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| {
            let what = match (p.role, p.level) {
                ("heading", Some(1)) => "Title",
                ("heading", _) => "Heading",
                ("figure", _) => "Picture",
                ("table", _) => "Table",
                _ => "Text",
            };
            format!("{what}: {}", p.text.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .collect();
    lines.join("\n")
}

/// A change of a text's words: the characters `from` to `to` (Unicode scalar values, as
/// `replace_text` counts them) become `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replacement {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// The least changes that make `before` read `after`, a word or a mark at a time, last first, so
/// each applies to the text the one before it left: words the answer keeps stay as they were,
/// with the looks their runs give them. None where the two are the same.
pub fn replacing(before: &str, after: &str) -> Vec<Replacement> {
    let a = tokens(before);
    let b = tokens(after);
    // The words both share at their starts and their ends.
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = (a[head..].iter().rev()).zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (am, bm) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    // Where each of `before`'s tokens starts, in characters.
    let mut starts = Vec::with_capacity(a.len() + 1);
    let mut at = 0;
    for t in &a {
        starts.push(at);
        at += t.chars().count();
    }
    starts.push(at);
    let mut hunks: Vec<(usize, usize, usize, usize)> = Vec::new();
    if am.is_empty() && bm.is_empty() {
        return Vec::new();
    }
    if am.len().saturating_mul(bm.len()) > 250_000 {
        // Too long to match word by word: the middle that differs, at once.
        hunks.push((0, am.len(), 0, bm.len()));
    } else {
        hunks = differ(am, bm);
    }
    hunks
        .into_iter()
        .rev()
        .map(|(a0, a1, b0, b1)| Replacement {
            from: starts[head + a0],
            to: starts[head + a1],
            text: bm[b0..b1].concat(),
        })
        .collect()
}

/// `text` as words, runs of spaces, and each other mark alone.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut kind: Option<u8> = None;
    for (i, c) in text.char_indices() {
        let k = if c.is_alphanumeric() || c == '\'' || c == '’' {
            0
        } else if c.is_whitespace() && c != '\n' {
            1
        } else {
            2
        };
        if kind != Some(k) || k == 2 {
            if i > start {
                out.push(&text[start..i]);
            }
            start = i;
        }
        kind = Some(k);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The spans of `a` and `b` that differ, in order, as `(a from, a to, b from, b to)`: what is
/// left of each when their longest common run of tokens is taken out.
fn differ(a: &[&str], b: &[&str]) -> Vec<(usize, usize, usize, usize)> {
    let (n, m) = (a.len(), b.len());
    // lcs[i][j]: the longest common run of a[i..] and b[j..].
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[at(i, j)] =
                if a[i] == b[j] { lcs[at(i + 1, j + 1)] + 1 } else { lcs[at(i + 1, j)].max(lcs[at(i, j + 1)]) };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            if let Some((a0, b0)) = open.take() {
                out.push((a0, i, b0, j));
            }
            i += 1;
            j += 1;
            continue;
        }
        open.get_or_insert((i, j));
        if j < m && (i == n || lcs[at(i, j + 1)] >= lcs[at(i + 1, j)]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if let Some((a0, b0)) = open {
        out.push((a0, n, b0, m));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `before` with `changes` applied in turn, as a patch of `replace_text`s applies them.
    fn applied(before: &str, changes: &[Replacement]) -> String {
        let mut chars: Vec<char> = before.chars().collect();
        for c in changes {
            chars.splice(c.from..c.to, c.text.chars());
        }
        chars.into_iter().collect()
    }

    #[test]
    fn a_proofread_changes_only_the_words_that_were_wrong() {
        let before = "Revenue grew stedily in the thrid quarter, its best since 2019.";
        let after = "Revenue grew steadily in the third quarter, its best since 2019.";
        let changes = replacing(before, after);
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert_eq!(applied(before, &changes), after);
        // Last first: each applies to the text the one before it left.
        assert!(changes[0].from > changes[1].from);
        assert_eq!(changes[1], Replacement { from: 13, to: 20, text: "steadily".into() });
    }

    #[test]
    fn a_tightened_text_reads_as_the_answer_and_keeps_its_lines() {
        let before = "We have made the decision to expand\ninto three new markets this year";
        let after = "We will expand\ninto three new markets this year";
        let changes = replacing(before, after);
        assert_eq!(applied(before, &changes), after);
        assert_eq!(changes.len(), 1);
        assert!(replacing(after, after).is_empty());
    }

    #[test]
    fn characters_count_as_replace_text_counts_them() {
        let before = "Café prices rose 5 % — again.";
        let after = "Café prices rose 5 % — yet again.";
        let changes = replacing(before, after);
        assert_eq!(applied(before, &changes), after);
        assert_eq!(changes, vec![Replacement { from: 23, to: 23, text: "yet ".into() }]);
    }

    #[test]
    fn an_answer_in_quotes_is_taken_without_them_where_the_text_had_none() {
        let tighten = Task::Tighten { text: "A long headline".into() };
        assert_eq!(tighten.taken("  “A headline”\n"), "A headline");
        assert_eq!(tighten.taken("\"A headline\""), "A headline");
        let quoted = Task::Proofread { text: "“Ship it,” she said".into() };
        assert_eq!(quoted.taken("“Ship it,” she said."), "“Ship it,” she said.");
        let notes = Task::Notes { slide: "Title: Q3".into(), claim: None };
        assert_eq!(notes.taken("\"Q3 was good.\"\r\nThanks."), "\"Q3 was good.\"\nThanks.");
    }

    #[test]
    fn each_task_asks_in_its_own_words() {
        let notes = Task::Notes { slide: "Title: Revenue doubled".into(), claim: Some("Growth is real".into()) };
        assert_eq!(notes.prompt(), "The slide reads:\nTitle: Revenue doubled\nThe point it makes: Growth is real");
        let explain = Task::Explain(Explained {
            code: "E100".into(),
            severity: "error".into(),
            message: "The words run out of their box.".into(),
            hint: Some("Shorten it".into()),
            words: Some("Volunteers rebuilt fifty-two miles".into()),
        });
        assert_eq!(
            explain.prompt(),
            "Finding E100 (error): The words run out of their box.\nLint's hint: Shorten it\nIts words: Volunteers rebuilt fifty-two miles"
        );
        let json = serde_json::to_value(&explain).unwrap();
        assert_eq!(json["kind"], "explain");
        assert_eq!(serde_json::from_value::<Task>(json).unwrap(), explain);
        let read: Task = serde_json::from_str(r#"{"kind":"tighten","text":"Words"}"#).unwrap();
        assert_eq!(read.prompt(), "Words");
        assert!(read.instructions().starts_with("You tighten"));
    }
}
