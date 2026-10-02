//! JSON Patch (RFC 6902) over a deck's JSON: `add`, `remove`, `replace`, `move`, `copy`,
//! and `test`, with JSON Pointer (RFC 6901) paths. A patch applies as a whole or not at
//! all: on any failure the document is left as it was (SPEC §7.3). Lint's fixes are
//! patches (SPEC §7.4); the semantic ops of `scaena patch` compile to them (PLAN 1.16).

use serde_json::Value;

/// Why a patch did not apply: which op, and what was wrong with it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("op {index}: {message}")]
pub struct PatchError {
    pub index: usize,
    pub message: String,
}

/// Apply `ops` to `doc`, all or none.
pub fn apply(doc: &mut Value, ops: &[Value]) -> Result<(), PatchError> {
    let mut work = doc.clone();
    for (index, op) in ops.iter().enumerate() {
        one(&mut work, op).map_err(|message| PatchError { index, message })?;
    }
    *doc = work;
    Ok(())
}

fn one(doc: &mut Value, op: &Value) -> Result<(), String> {
    let field = |k: &str| op.get(k).and_then(Value::as_str).ok_or_else(|| format!("needs a string `{k}`"));
    let value = || op.get("value").cloned().ok_or_else(|| "needs a `value`".to_string());
    let path = field("path")?;
    match field("op")? {
        "add" => add(doc, path, value()?),
        "remove" => remove(doc, path).map(drop),
        "replace" => {
            let target = doc.pointer_mut(path).ok_or_else(|| format!("`{path}` is not there"))?;
            *target = value()?;
            Ok(())
        }
        "move" => {
            let from = field("from")?;
            if path.starts_with(&format!("{from}/")) {
                return Err(format!("cannot move `{from}` into itself"));
            }
            let v = remove(doc, from)?;
            add(doc, path, v)
        }
        "copy" => {
            let from = field("from")?;
            let v = doc.pointer(from).cloned().ok_or_else(|| format!("`{from}` is not there"))?;
            add(doc, path, v)
        }
        "test" => match doc.pointer(path) {
            Some(v) if *v == value()? => Ok(()),
            Some(v) => Err(format!("`{path}` is {v}, not {}", value()?)),
            None => Err(format!("`{path}` is not there")),
        },
        other => Err(format!("`{other}` is not a JSON Patch op")),
    }
}

/// The parent of `path` and its last token, unescaped.
fn split(path: &str) -> Result<(&str, String), String> {
    let (parent, last) = path.rsplit_once('/').ok_or_else(|| format!("`{path}` is not a JSON pointer"))?;
    Ok((parent, last.replace("~1", "/").replace("~0", "~")))
}

fn add(doc: &mut Value, path: &str, value: Value) -> Result<(), String> {
    if path.is_empty() {
        *doc = value;
        return Ok(());
    }
    let (parent, key) = split(path)?;
    match doc.pointer_mut(parent) {
        Some(Value::Object(m)) => {
            m.insert(key, value);
            Ok(())
        }
        Some(Value::Array(a)) => {
            let i = if key == "-" { a.len() } else { index(&key, a.len() + 1)? };
            a.insert(i, value);
            Ok(())
        }
        Some(_) => Err(format!("`{parent}` holds neither an object nor an array")),
        None => Err(format!("`{parent}` is not there")),
    }
}

fn remove(doc: &mut Value, path: &str) -> Result<Value, String> {
    let (parent, key) = split(path)?;
    match doc.pointer_mut(parent) {
        Some(Value::Object(m)) => m.shift_remove(&key).ok_or_else(|| format!("`{path}` is not there")),
        Some(Value::Array(a)) => {
            let i = index(&key, a.len())?;
            Ok(a.remove(i))
        }
        _ => Err(format!("`{path}` is not there")),
    }
}

/// An array index token below `len`: digits, no leading zero.
fn index(key: &str, len: usize) -> Result<usize, String> {
    let ok = !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()) && (key == "0" || !key.starts_with('0'));
    let i: usize = key.parse().ok().filter(|_| ok).ok_or_else(|| format!("`{key}` is not an array index"))?;
    if i < len { Ok(i) } else { Err(format!("index {i} is past the end")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_op_applies_as_rfc_6902_says() {
        let mut doc = json!({ "a": { "b": 1 }, "list": [1, 2, 3], "a/b": 0 });
        let ops = [
            json!({ "op": "add", "path": "/a/c", "value": 2 }),
            json!({ "op": "add", "path": "/list/1", "value": 9 }),
            json!({ "op": "add", "path": "/list/-", "value": 4 }),
            json!({ "op": "replace", "path": "/a/b", "value": 10 }),
            json!({ "op": "remove", "path": "/list/0" }),
            json!({ "op": "copy", "from": "/a", "path": "/copied" }),
            json!({ "op": "move", "from": "/a/c", "path": "/moved" }),
            json!({ "op": "test", "path": "/a~1b", "value": 0 }),
        ];
        apply(&mut doc, &ops).unwrap();
        assert_eq!(
            doc,
            json!({ "a": { "b": 10 }, "list": [9, 2, 3, 4], "a/b": 0, "copied": { "b": 10, "c": 2 }, "moved": 2 })
        );
    }

    #[test]
    fn a_patch_that_fails_changes_nothing() {
        let before = json!({ "a": 1, "list": [1] });
        for ops in [
            vec![json!({ "op": "replace", "path": "/a", "value": 2 }), json!({ "op": "remove", "path": "/missing" })],
            vec![json!({ "op": "test", "path": "/a", "value": 2 })],
            vec![json!({ "op": "add", "path": "/list/5", "value": 0 })],
            vec![json!({ "op": "add", "path": "/list/01", "value": 0 })],
            vec![json!({ "op": "move", "from": "/list", "path": "/list/0" })],
            vec![json!({ "op": "frobnicate", "path": "/a" })],
            vec![json!({ "op": "add", "path": "/nowhere/x", "value": 0 })],
        ] {
            let mut doc = before.clone();
            assert!(apply(&mut doc, &ops).is_err(), "{ops:?}");
            assert_eq!(doc, before, "{ops:?}");
        }
        let mut doc = before.clone();
        let err = apply(
            &mut doc,
            &[json!({ "op": "add", "path": "/b", "value": 1 }), json!({ "op": "remove", "path": "/c" })],
        );
        assert_eq!(err.unwrap_err().index, 1);
    }
}
