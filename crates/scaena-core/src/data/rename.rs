//! A data source's column renamed through the deck (R1's dogfood 10): the source's `schema` and
//! `parse` for it, its rows where they are written inline, and every reading of it, a chart's or
//! a table's fields and its `dataTransform`, and each run's `quote`, so a header renamed in its
//! file leaves no chart reading a column that is no longer there. The file's own header is
//! `data::edit`'s, which a `rename` edit writes beside this.

use crate::expr;
use serde_json::{Map, Value, json};

/// The JSON Patch that renames column `from` of source `name` to `to` through the deck `doc`:
/// a `replace` of each part it changes, the source's declaration, each node, and each state's
/// props for a node. Why not, where a transform's expression does not read, or makes a column
/// named `to` that the rename would read as the source's.
pub fn renaming(doc: &Value, name: &str, from: &str, to: &str) -> Result<Vec<Value>, String> {
    let source = format!("@{name}");
    let mut ops = Vec::new();
    if let Some(declared) = doc.pointer(&format!("/data/{}", token(name))) {
        let mut declared = declared.clone();
        for key in ["schema", "parse"] {
            if let Some(Value::Object(map)) = declared.get_mut(key) {
                rekey(map, from, to);
            }
        }
        if let Some(Value::Array(rows)) = declared.get_mut("source").and_then(|s| s.get_mut("inline")) {
            for row in rows.iter_mut().filter_map(Value::as_object_mut) {
                rekey(row, from, to);
            }
        }
        if Some(&declared) != doc.pointer(&format!("/data/{}", token(name))) {
            ops.push(json!({ "op": "replace", "path": format!("/data/{}", token(name)), "value": declared }));
        }
    }
    let nodes = doc.get("nodes").and_then(Value::as_object);
    let states = doc.get("states").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    // The nodes that read the source: in their own props, or in any state's.
    let reads = |id: &str, node: &Value| {
        node.get("data").and_then(Value::as_str) == Some(&source)
            || states.iter().any(|s| {
                s.get("props").and_then(|p| p.get(id)).and_then(|p| p.get("data")).and_then(Value::as_str)
                    == Some(&source)
            })
    };
    for (id, node) in nodes.into_iter().flatten() {
        let mut renamed = node.clone();
        if reads(id, node) {
            let own = node.get("dataTransform").cloned();
            reader(&mut renamed, own.as_ref(), from, to)?;
        }
        quotes(&mut renamed, &source, from, to)?;
        if &renamed != node {
            ops.push(json!({ "op": "replace", "path": format!("/nodes/{}", token(id)), "value": renamed }));
        }
    }
    for (i, state) in states.iter().enumerate() {
        for (id, props) in state.get("props").and_then(Value::as_object).into_iter().flatten() {
            let mut renamed = props.clone();
            if let Some(node) = nodes.and_then(|n| n.get(id))
                && reads(id, node)
            {
                // The transform the state reads through: its own, else its node's.
                let through = props.get("dataTransform").or_else(|| node.get("dataTransform")).cloned();
                reader(&mut renamed, through.as_ref(), from, to)?;
            }
            quotes(&mut renamed, &source, from, to)?;
            if &renamed != props {
                let path = format!("/states/{i}/props/{}", token(id));
                ops.push(json!({ "op": "replace", "path": path, "value": renamed }));
            }
        }
    }
    Ok(ops)
}

/// A chart's or a table's props, as a node or a state writes them, with `from` read as `to`: its
/// `dataTransform`, `through` the transform it reads, and its fields where the column reaches
/// them through it.
fn reader(props: &mut Value, through: Option<&Value>, from: &str, to: &str) -> Result<(), String> {
    let Some(props) = props.as_object_mut() else { return Ok(()) };
    let reaches = match through.and_then(Value::as_array) {
        Some(steps) => transformed(steps, from, to)?.1,
        None => true,
    };
    if let Some(Value::Array(steps)) = props.get_mut("dataTransform") {
        *steps = transformed(steps, from, to)?.0;
    }
    if !reaches {
        return Ok(());
    }
    let field = |v: &mut Value| {
        if v.as_str() == Some(from) {
            *v = Value::String(to.to_string());
        }
    };
    if let Some(key) = props.get_mut("key") {
        field(key);
    }
    for channel in ["x", "y", "series", "color", "sizeEncoding", "projected", "facet"] {
        if let Some(f) = props.get_mut(channel).and_then(|c| c.get_mut("field")) {
            field(f);
        }
    }
    for column in props.get_mut("columns").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(f) = column.get_mut("field") {
            field(f);
        }
    }
    for end in ["low", "high"] {
        if let Some(f) = props.get_mut("interval").and_then(|i| i.get_mut(end)) {
            field(f);
        }
    }
    let parts = props.get_mut("weather").and_then(|w| w.get_mut("fields")).and_then(Value::as_object_mut);
    for f in parts.into_iter().flat_map(|p| p.values_mut()) {
        field(f);
    }
    Ok(())
}

/// Each run's `quote` of the source in `value`, with `from` read as `to`: its transform, and
/// its column and its row's keys where the column reaches them.
fn quotes(value: &mut Value, source: &str, from: &str, to: &str) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            let quoted = map.get("data").and_then(Value::as_str) == Some(source)
                && map.get("column").and_then(Value::as_str).is_some();
            if quoted {
                let mut reaches = true;
                if let Some(Value::Array(steps)) = map.get_mut("dataTransform") {
                    let (renamed, through) = transformed(steps, from, to)?;
                    *steps = renamed;
                    reaches = through;
                }
                if reaches {
                    if map.get("column").and_then(Value::as_str) == Some(from) {
                        map.insert("column".into(), Value::String(to.to_string()));
                    }
                    if let Some(Value::Object(row)) = map.get_mut("row") {
                        rekey(row, from, to);
                    }
                }
                return Ok(());
            }
            for v in map.values_mut() {
                quotes(v, source, from, to)?;
            }
            Ok(())
        }
        Value::Array(items) => items.iter_mut().try_for_each(|v| quotes(v, source, from, to)),
        _ => Ok(()),
    }
}

/// `steps` with the source's column `from` read as `to` wherever it reaches, and whether it
/// reaches the table they make, as the same column: a step that makes a column of its name, or
/// folds, aggregates, or pivots it away, ends it there. A step that makes a column named `to`
/// while the column still reaches it is refused: the two would be one name.
fn transformed(steps: &[Value], from: &str, to: &str) -> Result<(Vec<Value>, bool), String> {
    let mut live = true;
    let mut out = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let mut step = step.clone();
        let Some(o) = step.as_object_mut() else {
            out.push(step);
            continue;
        };
        let clash = |name: &str| -> Result<(), String> {
            match name == to {
                true => Err(format!("step {i} of a `dataTransform` makes a column `{to}` already")),
                false => Ok(()),
            }
        };
        let expression = |v: &mut Value| -> Result<(), String> {
            if let Some(src) = v.as_str() {
                let renamed = expr::renamed(src, from, to).map_err(|e| format!("step {i}: `{src}` {e}"))?;
                *v = Value::String(renamed);
            }
            Ok(())
        };
        let fields = |v: &mut Value, prefix: bool| {
            let one = |s: &mut Value| {
                let Some(text) = s.as_str() else { return };
                let (sign, bare) = match prefix {
                    true => text.strip_prefix('-').map_or(("", text), |b| ("-", b)),
                    false => ("", text),
                };
                if bare == from {
                    *s = Value::String(format!("{sign}{to}"));
                }
            };
            match v {
                Value::Array(items) => items.iter_mut().for_each(one),
                v => one(v),
            }
        };
        let holds = |v: Option<&Value>, name: &str| match v {
            Some(Value::Array(items)) => items.iter().any(|s| s.as_str() == Some(name)),
            Some(v) => v.as_str() == Some(name),
            None => false,
        };
        if live {
            if let Some(v) = o.get_mut("filter") {
                expression(v)?;
            }
            if let Some(Value::Object(derived)) = o.get_mut("derive") {
                for (name, v) in derived.iter_mut() {
                    if live {
                        expression(v)?;
                        clash(name)?;
                    }
                    live &= name != from;
                }
            }
            if let Some(v) = o.get_mut("sort") {
                fields(v, true);
            }
            if let Some(Value::Object(specs)) = o.get_mut("aggregate") {
                for (name, v) in specs.iter_mut() {
                    expression(v)?;
                    clash(name)?;
                }
            }
            if let Some(v) = o.get_mut("fold") {
                fields(v, false);
            }
            for key in ["pivot", "value", "groupby"] {
                if let Some(v) = o.get_mut(key) {
                    fields(v, false);
                }
            }
            if let Some(names) = o.get("as") {
                for name in names.as_array().into_iter().flatten().filter_map(Value::as_str) {
                    clash(name)?;
                }
            }
            // What keeps the column past the step: a fold that folds it, an aggregate or a pivot
            // that does not group by it, ends it.
            if o.contains_key("fold") && holds(o.get("fold"), to) {
                live = false;
            }
            if (o.contains_key("aggregate") || o.contains_key("pivot")) && !holds(o.get("groupby"), to) {
                live = false;
            }
        }
        out.push(step);
    }
    Ok((out, live))
}

/// `map` with its key `from` named `to`, its place kept.
fn rekey(map: &mut Map<String, Value>, from: &str, to: &str) {
    if !map.contains_key(from) {
        return;
    }
    for (k, v) in std::mem::take(map) {
        map.insert(if k == from { to.to_string() } else { k }, v);
    }
}

/// A JSON Pointer's token for `s`.
fn token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transform_reads_the_column_by_its_new_name_while_it_reaches() {
        let steps = vec![
            json!({ "filter": "margin > 0 && `margin` < 1" }),
            json!({ "sort": ["-margin", "quarter"] }),
            json!({ "derive": { "pct": "margin * 100" } }),
        ];
        let (renamed, live) = transformed(&steps, "margin", "gross margin").unwrap();
        assert!(live);
        assert_eq!(renamed[0], json!({ "filter": "`gross margin` > 0 && `gross margin` < 1" }));
        assert_eq!(renamed[1], json!({ "sort": ["-gross margin", "quarter"] }));
        assert_eq!(renamed[2], json!({ "derive": { "pct": "`gross margin` * 100" } }));
        // An aggregate that does not group by it ends it; one that groups by it keeps it.
        let gone = [json!({ "aggregate": { "total": "sum(margin)" }, "groupby": ["quarter"] })];
        let (renamed, live) = transformed(&gone, "margin", "gm").unwrap();
        assert_eq!((renamed[0]["aggregate"]["total"].clone(), live), (json!("sum(gm)"), false));
        let kept = [json!({ "aggregate": { "n": "count()" }, "groupby": ["margin"] })];
        assert!(transformed(&kept, "margin", "gm").unwrap().1);
        // A derive that makes the new name already is refused; a function's name is no field.
        assert!(transformed(&[json!({ "derive": { "gm": "1" } })], "margin", "gm").is_err());
        assert_eq!(expr::renamed("round(round)", "round", "r").unwrap(), "round(r)");
        assert_eq!(expr::renamed("'margin' == margin", "margin", "m").unwrap(), "'margin' == m");
    }
}
