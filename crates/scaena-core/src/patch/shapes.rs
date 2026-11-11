//! Each semantic op's shape in a line (agent dogfood 2): what an agent writes, required keys first
//! and optional ones marked `?`, a value in capitals its kind. An op that does not read says its
//! shape after why, and the author-deck skill lists the same lines, which `skills.rs` holds to
//! these; `docs/schema/patch.schema.json` is the whole of it.

/// Each semantic op by its name, and its shape.
pub const SHAPES: &[(&str, &str)] = &[
    ("add_node", r#"{ "op": "add_node", "id": ID, "node": NODE, "state"?: STATE, "props"?: PROPS, "after"?: ID }"#),
    ("remove_node", r#"{ "op": "remove_node", "id": ID }"#),
    ("rename_node", r#"{ "op": "rename_node", "id": ID, "to": ID }"#),
    ("show_node", r#"{ "op": "show_node", "node": ID, "state": STATE, "props"?: PROPS }"#),
    ("hide_node", r#"{ "op": "hide_node", "node": ID, "state": STATE }"#),
    ("delete_node", r#"{ "op": "delete_node", "node": ID, "state": STATE, "everywhere"?: BOOL }"#),
    ("group", r#"{ "op": "group", "id": ID, "nodes": [ID, …], "state"?: STATE }"#),
    ("ungroup", r#"{ "op": "ungroup", "group": ID }"#),
    ("set_prop", r#"{ "op": "set_prop", "node": ID, "prop": "PROP" | "PROP/KEY", "value": VALUE, "state"?: STATE }"#),
    (
        "place",
        r#"{ "op": "place", "node": ID, "at": AT, "state"?: STATE, "fork"?: BOOL, "format"?: FORMAT, "anew"?: BOOL }"#,
    ),
    ("set_text", r#"{ "op": "set_text", "node": ID, "text": TEXT, "state"?: STATE }"#),
    (
        "replace_text",
        r#"{ "op": "replace_text", "node": ID, "from": CHAR, "to": CHAR, "text": TEXT, "state"?: STATE, "fork"?: BOOL }"#,
    ),
    (
        "style_text",
        r#"{ "op": "style_text", "node": ID, "from": CHAR, "to": CHAR, "look": LOOK, "state"?: STATE, "fork"?: BOOL }"#,
    ),
    (
        "list",
        r#"{ "op": "list", "node": ID, "from": CHAR, "to": CHAR, "kind"?: "bullet" | "number" | null, "level"?: N, "by"?: N, "state"?: STATE, "fork"?: BOOL }"#,
    ),
    (
        "choose",
        r#"{ "op": "choose", "node": ID, "prop": "PROP" | "PROP/KEY", "value": VALUE, "state"?: STATE, "fork"?: BOOL }"#,
    ),
    (
        "annotate",
        r#"{ "op": "annotate", "node": ID, "index"?: N, "annotation"?: ANNOTATION | null, "state"?: STATE, "fork"?: BOOL }"#,
    ),
    ("bind_data", r#"{ "op": "bind_data", "node": ID, "data": "@SOURCE", "source"?: SOURCE, "state"?: STATE }"#),
    ("apply_preset", r#"{ "op": "apply_preset", "node": ID, "preset": PRESET, "motion"?: MOTION, "state"?: STATE }"#),
    (
        "time_motion",
        r#"{ "op": "time_motion", "node": ID, "motion": MOTION, "state": STATE, "delay"?: MS, "duration"?: DURATION }"#,
    ),
    (
        "add_state",
        r#"{ "op": "add_state", "state": { "id": STATE, … }, "after"?: STATE, "before"?: STATE, "beat"?: BEAT }"#,
    ),
    ("move_state", r#"{ "op": "move_state", "id": STATE, "after"?: STATE, "before"?: STATE }"#),
    ("remove_state", r#"{ "op": "remove_state", "id": STATE }"#),
    ("move_slide", r#"{ "op": "move_slide", "slide": STATE, "after"?: STATE, "before"?: STATE }"#),
    ("duplicate_slide", r#"{ "op": "duplicate_slide", "slide": STATE }"#),
    ("remove_slide", r#"{ "op": "remove_slide", "slide": STATE }"#),
    ("rename_state", r#"{ "op": "rename_state", "id": STATE, "to": STATE }"#),
    (
        "set_state",
        r#"{ "op": "set_state", "id": STATE, "prop": "layout" | "hold" | "notes" | "furniture" | "transition" | "transition/KEY", "value": VALUE, "fork"?: BOOL }"#,
    ),
    ("furnish", r#"{ "op": "furnish", "number"?: BOOL, "footer"?: TEXT }"#),
    ("retheme", r#"{ "op": "retheme", "theme": THEME }"#),
];

/// The shape of the semantic op `name`, where there is one by that name.
pub fn shape(name: &str) -> Option<&'static str> {
    SHAPES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Each semantic op the patch schema has has a shape, and each shape names the op's keys as the
    /// schema does: each it requires as written, each it does not marked `?`, and none other.
    #[test]
    fn every_op_has_its_shape_as_the_schema_writes_it() {
        let schema: Value = serde_json::from_str(include_str!("../../../../docs/schema/patch.schema.json")).unwrap();
        let ops = schema["$defs"]["SemanticOp"]["oneOf"].as_array().unwrap();
        assert_eq!(ops.len(), SHAPES.len(), "one shape for each op");
        for op in ops {
            let name = op["properties"]["op"]["const"].as_str().or(op["properties"]["op"]["enum"][0].as_str()).unwrap();
            let shape = shape(name).unwrap_or_else(|| panic!("`{name}` has no shape"));
            assert!(shape.starts_with(&format!(r#"{{ "op": "{name}""#)), "{shape}");
            let required: Vec<&str> =
                op["required"].as_array().unwrap().iter().filter_map(Value::as_str).filter(|k| *k != "op").collect();
            let keys: Vec<&str> =
                op["properties"].as_object().unwrap().keys().map(String::as_str).filter(|k| *k != "op").collect();
            for key in &keys {
                let mark = if required.contains(key) { format!(r#""{key}": "#) } else { format!(r#""{key}"?: "#) };
                assert!(shape.contains(&mark), "`{name}`'s `{key}` as {mark:?} in {shape}");
            }
            // No key the schema lacks, as the top level writes them.
            let written = shape.matches(r#"": "#).count() + shape.matches(r#""?: "#).count();
            let nested = shape.matches(r#"{ "id": STATE"#).count();
            assert_eq!(written - nested, keys.len() + 1, "`{name}`: {shape}");
        }
    }

    /// An op that does not read says the shape it is written in (agent dogfood 2).
    #[test]
    fn an_op_that_does_not_read_says_its_shape() {
        let wrong = serde_json::json!({ "op": "set_state", "state": "s", "notes": "Hello" });
        let error = crate::patch::Op::from_value(wrong).err().unwrap();
        assert!(error.contains("unknown field `state`"), "{error}");
        assert!(error.ends_with(&format!("; `set_state` is written {}", shape("set_state").unwrap())), "{error}");
        assert!(error.contains(r#""prop": "layout" | "hold" | "notes""#), "{error}");
    }
}
