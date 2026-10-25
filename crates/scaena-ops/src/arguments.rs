//! A tool's arguments read as the type it takes, an argument that does not read named by its
//! field (dogfood 3): "`size`: invalid type: sequence, expected a string", not the bare reason.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// `args` read as `T`. Where one does not read, the reason names the argument it is in, as
/// serde names a field missing or unknown already.
pub fn read<T: DeserializeOwned>(args: Map<String, Value>) -> Result<T, String> {
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
}
