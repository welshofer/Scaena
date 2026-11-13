//! A tool's arguments read as the type it takes, an argument that does not read named by its
//! field (dogfood 3): "`size`: invalid type: sequence, expected a string", not the bare reason.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// `args` read as `T`. Where one does not read, the reason names the argument it is in, as
/// serde names a field missing or unknown already.
pub fn read<T: DeserializeOwned>(args: Map<String, Value>) -> Result<T, String> {
    read_shaped(args, None)
}

/// `args` read as [`read`] reads them, a struct the reason names by its Rust name ("expected
/// struct Attach") said by the shape `schema` gives it, its fields, each optional one marked:
/// "expected an object `{ id, file, schema?, parse? }`" (R1's agent dogfood 11).
pub fn read_shaped<T: DeserializeOwned>(args: Map<String, Value>, schema: Option<&Value>) -> Result<T, String> {
    read_named(args).map_err(|reason| match schema {
        Some(schema) => shaped(&reason, schema),
        None => reason,
    })
}

/// `reason` with each `struct NAME` it expects said as the shape `schema`'s definition of NAME
/// gives it.
fn shaped(reason: &str, schema: &Value) -> String {
    let Some(at) = reason.find("expected struct ") else { return reason.to_string() };
    let rest = &reason[at + "expected struct ".len()..];
    let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    let def = ["$defs", "definitions"].iter().find_map(|defs| schema.get(*defs).and_then(|d| d.get(&name)));
    let Some(fields) = def.and_then(|d| d.get("properties")).and_then(Value::as_object) else {
        return reason.to_string();
    };
    let required: Vec<&str> = (def.and_then(|d| d.get("required")).and_then(Value::as_array).into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let fields: Vec<String> =
        fields.keys().map(|k| if required.contains(&k.as_str()) { k.clone() } else { format!("{k}?") }).collect();
    format!("{}expected an object `{{ {} }}`{}", &reason[..at], fields.join(", "), &rest[name.len()..])
}

fn read_named<T: DeserializeOwned>(args: Map<String, Value>) -> Result<T, String> {
    let args = Value::Object(args);
    let reason = match serde_json::from_value::<T>(args.clone()) {
        Ok(read) => return Ok(read),
        Err(e) => e.to_string(),
    };
    if reason.contains("field `") {
        return Err(reason);
    }
    match field(&args, |text| serde_json::from_str::<T>(text).err().map(|e| e.line())) {
        Some(field) => Err(format!("`{field}`: {reason}")),
        None => Err(reason),
    }
}

/// The argument of `args` that `fails` stops in, read from the line it stops on with each
/// argument written from a line of its own: the last line before it, or it, that begins one.
fn field(args: &Value, fails: impl Fn(&str) -> Option<usize>) -> Option<String> {
    let text = serde_json::to_string_pretty(args).ok()?;
    let line = fails(&text)?;
    text.lines().take(line).collect::<Vec<_>>().into_iter().rev().find_map(|l| {
        let key = l.strip_prefix("  \"")?;
        let end = key.find("\": ")?;
        serde_json::from_str(&format!("\"{}\"", &key[..end])).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Args {
        bundle: String,
        #[serde(default)]
        size: Option<String>,
        #[serde(default)]
        ops: Vec<Map<String, Value>>,
    }

    fn reason(args: Value) -> String {
        let Value::Object(args) = args else { unreachable!() };
        read::<Args>(args).unwrap_err()
    }

    #[test]
    fn an_argument_that_does_not_read_is_named() {
        assert_eq!(
            reason(json!({ "bundle": "deck", "size": [960, 540] })),
            "`size`: invalid type: sequence, expected a string"
        );
        // Inside an argument, the argument it is in.
        assert!(reason(json!({ "bundle": "d", "ops": [{ "op": "add" }, 7] })).starts_with("`ops`: "));
        // Serde names a field missing or unknown itself.
        assert_eq!(reason(json!({ "size": "960" })), "missing field `bundle`");
        assert!(reason(json!({ "bundle": "d", "sise": "960" })).starts_with("unknown field `sise`"));
        let Value::Object(fine) = json!({ "bundle": "d", "size": "1080p" }) else { unreachable!() };
        assert_eq!(read::<Args>(fine).unwrap().size.as_deref(), Some("1080p"));
    }

    #[derive(Debug, Deserialize, schemars::JsonSchema)]
    #[allow(dead_code)]
    struct Attach {
        id: String,
        file: String,
        #[serde(default)]
        schema: Option<Map<String, Value>>,
    }

    #[derive(Debug, Deserialize, schemars::JsonSchema)]
    #[allow(dead_code)]
    struct Create {
        bundle: String,
        #[serde(default)]
        data: Vec<Attach>,
    }

    /// A struct an argument holds is said by the shape the tool's schema gives it, its optional
    /// fields marked, never by its Rust name (R1's agent dogfood 11: `deck_create`'s `data`).
    #[test]
    fn a_struct_that_does_not_read_is_said_by_its_shape() {
        let schema = serde_json::to_value(schemars::schema_for!(Create)).unwrap();
        let Value::Object(args) = json!({ "bundle": "d", "data": ["ci.csv"] }) else { unreachable!() };
        assert_eq!(
            read_shaped::<Create>(args.clone(), Some(&schema)).unwrap_err(),
            "`data`: invalid type: string \"ci.csv\", expected an object `{ id, file, schema? }`"
        );
        assert_eq!(
            read::<Create>(args).unwrap_err(),
            "`data`: invalid type: string \"ci.csv\", expected struct Attach",
            "without a schema, as serde says it"
        );
    }
}
