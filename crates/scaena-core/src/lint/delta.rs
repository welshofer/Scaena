//! What a change does to what lint finds (SPEC §7.1, §7.3): the findings it adds and the
//! ones it removes. `theme --apply` and `patch` report this, and `patch` refuses a change
//! that adds a validation error.

use super::Finding;
use crate::patch::Renamed;
use std::collections::HashMap;

/// The findings a change adds, and the ones it removes.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta<'a> {
    pub added: Vec<&'a Finding>,
    pub removed: Vec<&'a Finding>,
}

/// What lint found before a change and after it, compared; `was` and `is` are the deck's
/// state ids, in order, before and after. A finding
/// is the same one before and after when it is about the same thing:
/// - the same code, file, and format;
/// - the same state and node, a state named by its id where its path names it by place (a
///   state added before it moves it), and ids the change renamed read by their new names;
/// - the same message but for its figures: a widow that gains a word, or an overlap that
///   grows, is still the same widow or overlap.
///
/// Findings are counted, so a second finding just like one already there is added.
pub fn delta<'a>(
    before: &'a [Finding],
    was: &[&str],
    after: &'a [Finding],
    is: &[&str],
    renamed: &[Renamed],
) -> Delta<'a> {
    let old: Vec<Key> = before.iter().map(|f| key(f, was, renamed)).collect();
    let new: Vec<Key> = after.iter().map(|f| key(f, is, &[])).collect();
    Delta { added: unmatched(after, &new, &old), removed: unmatched(before, &old, &new) }
}

/// The findings of `these` (keyed `keys`) that `others` holds no match for, each match
/// counted once.
fn unmatched<'a>(these: &'a [Finding], keys: &[Key], others: &[Key]) -> Vec<&'a Finding> {
    let mut left: HashMap<&Key, usize> = HashMap::new();
    for k in others {
        *left.entry(k).or_default() += 1;
    }
    these
        .iter()
        .zip(keys)
        .filter(|(_, k)| match left.get_mut(k) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .map(|(f, _)| f)
        .collect()
}

type Key = (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String);

fn key(f: &Finding, states: &[&str], renamed: &[Renamed]) -> Key {
    let node = |id: &str| {
        renamed.iter().fold(id.to_string(), |id, r| match r {
            Renamed::Node { from, to } if *from == id => to.clone(),
            _ => id,
        })
    };
    let state = |id: &str| {
        renamed.iter().fold(id.to_string(), |id, r| match r {
            Renamed::State { from, to } if *from == id => to.clone(),
            _ => id,
        })
    };
    // A path into the deck names a state by place: by its id instead, and every id by its
    // new name. A path into another file (a theme) holds no ids of the deck's.
    let path = f.path.as_deref().map(|path| {
        if f.file.is_some() {
            return path.to_string();
        }
        let mut tokens: Vec<String> = path.split('/').map(String::from).collect();
        for k in 1..tokens.len() {
            let slot = match (k, tokens[k - 1].as_str()) {
                (2, "states") => Slot::State,
                (2, "nodes" | "overrides") | (4, "props") => Slot::Node,
                _ => continue,
            };
            tokens[k] = match slot {
                Slot::State => match tokens[k].parse::<usize>().ok().and_then(|i| states.get(i)) {
                    Some(id) => id.to_string(),
                    None => state(&tokens[k]),
                },
                Slot::Node => node(&tokens[k]),
            };
        }
        tokens.join("/")
    });
    let mut message = f.message.clone();
    for r in renamed {
        let (Renamed::Node { from, to } | Renamed::State { from, to }) = r;
        message = message.replace(&format!("`{from}`"), &format!("`{to}`"));
    }
    (
        f.code.clone(),
        f.file.clone(),
        f.format.clone(),
        f.state.as_deref().map(state),
        f.node.as_deref().map(node),
        path,
        figureless(&message),
    )
}

enum Slot {
    State,
    Node,
}

/// `message` with each number in it, a decimal whole, as `#`. What is in backticks (an id,
/// a name, a value written in the deck) stays.
fn figureless(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        quoted ^= c == '`';
        if quoted || !c.is_ascii_digit() {
            out.push(c);
            continue;
        }
        while let Some(&d) = chars.peek() {
            let decimal = d == '.' && {
                let mut ahead = chars.clone();
                ahead.next();
                ahead.peek().is_some_and(char::is_ascii_digit)
            };
            if d.is_ascii_digit() || decimal {
                chars.next();
            } else {
                break;
            }
        }
        out.push('#');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::Severity;

    fn finding(code: &str, path: &str, message: &str) -> Finding {
        Finding::new(code, Severity::Warning, message).at(path)
    }

    #[test]
    fn a_state_is_known_by_its_id_and_a_message_by_its_words() {
        let (was, is) = (["a", "b"], ["new", "a", "b"]);
        let before = [
            finding("W210", "/states/1", "state `b` shows 45 words; the theme allows 40"),
            finding("W200", "/nodes/t", "`t` ends on 1 word"),
        ];
        let after = [
            finding("W210", "/states/2", "state `b` shows 47 words; the theme allows 40"),
            finding("W200", "/nodes/t", "`t` ends on 1 word"),
            finding("W200", "/nodes/t", "`t` ends on 1 word"),
        ];
        let d = delta(&before, &was, &after, &is, &[]);
        assert_eq!(d.removed, Vec::<&Finding>::new());
        assert_eq!(d.added, [&after[2]], "a second finding like one already there is new");
    }

    #[test]
    fn a_renamed_id_is_read_by_its_new_name() {
        let (was, is) = (["a"], ["a"]);
        let before = [finding("E101", "/states/0/props/rev", "`note` and `rev` overlap by 12 × 4.5 cu").node("rev")];
        let after =
            [finding("E101", "/states/0/props/chart", "`note` and `chart` overlap by 13 × 4.5 cu").node("chart")];
        let renamed = [Renamed::Node { from: "rev".into(), to: "chart".into() }];
        let d = delta(&before, &was, &after, &is, &renamed);
        assert_eq!((d.added.len(), d.removed.len()), (0, 0), "{d:#?}");
        // Without the rename, they are two findings.
        let d = delta(&before, &was, &after, &is, &[]);
        assert_eq!((d.added.len(), d.removed.len()), (1, 1));
    }

    #[test]
    fn figures_go_and_words_stay() {
        assert_eq!(figureless("3.2:1 below 4.5:1 at 24 px."), "#:# below #:# at # px.");
        assert_eq!(figureless("state `q3` and v1.2 in `@q4`"), "state `q3` and v# in `@q4`");
    }
}
