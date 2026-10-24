//! The shapes the model's answers must take (Narrato's `SLIDE_RESPONSE_SCHEMA` and
//! `ARC_SELECT_RESPONSE_SCHEMA`), in each provider's form. Gemini takes them as Vzestup writes
//! them, an OpenAPI subset with upper-case types. Anthropic takes JSON Schema derived from that,
//! as Vzestup derives it, every object closed. OpenAI's strict mode also wants every property
//! listed as required, so an optional one there may be null, which the answer reads as absent.

use serde_json::{Map, Value, json};

/// The slides' contract as Gemini's `responseSchema` takes it, as Vzestup writes it, with
/// Scaena's `icons` after Vzestup's fields: a word for each bullet naming its pictogram (PLAN
/// G.23), which no field asks for in Vzestup; and its `sourceRef`, the source a slide's facts come
/// from (narrative FUNC-3).
pub fn slides_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "executiveSummary": { "type": "STRING" },
            "slides": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "slideTitle": { "type": "STRING" },
                        "subtitle": { "type": "STRING" },
                        "layoutSuggestion": { "type": "STRING" },
                        "keyBullets": { "type": "ARRAY", "items": { "type": "STRING" } },
                        "visualDescription": { "type": "STRING" },
                        "speakerNotes": { "type": "STRING" },
                        "supportingFacts": { "type": "STRING" },
                        "chartData": {
                            "type": "OBJECT",
                            "properties": {
                                "categories": { "type": "ARRAY", "items": { "type": "STRING" } },
                                "series": {
                                    "type": "ARRAY",
                                    "items": {
                                        "type": "OBJECT",
                                        "properties": {
                                            "name": { "type": "STRING" },
                                            "values": { "type": "ARRAY", "items": { "type": "NUMBER" } }
                                        },
                                        "required": ["values"]
                                    }
                                },
                                "unit": { "type": "STRING" },
                                "sourceId": { "type": "STRING" }
                            },
                            "required": ["categories", "series"]
                        },
                        "outlineIndex": { "type": "NUMBER" },
                        "beatIndex": { "type": "NUMBER" },
                        "beatName": { "type": "STRING" },
                        "purpose": { "type": "STRING" },
                        "intent": { "type": "STRING" },
                        "expectedIntensity": { "type": "NUMBER" },
                        "expectedValence": { "type": "NUMBER" },
                        "icons": { "type": "ARRAY", "items": { "type": "STRING" } },
                        "sourceRef": { "type": "STRING" }
                    },
                    "required": [
                        "slideTitle", "subtitle", "layoutSuggestion", "keyBullets", "visualDescription",
                        "speakerNotes", "beatIndex", "beatName", "purpose", "intent", "expectedIntensity",
                        "expectedValence"
                    ]
                }
            }
        },
        "required": ["executiveSummary", "slides"]
    })
}

/// Arc selection's contract as Gemini takes it.
pub fn arc_gemini() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "selection": { "type": "STRING" },
            "reasoning": { "type": "STRING" },
            "candidates": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "arc": { "type": "STRING" },
                        "score": {
                            "type": "OBJECT",
                            "properties": {
                                "promptFit": { "type": "NUMBER" },
                                "goalFit": { "type": "NUMBER" },
                                "audienceFit": { "type": "NUMBER" },
                                "beatFit": { "type": "NUMBER" },
                                "intensityFit": { "type": "NUMBER" },
                                "overall": { "type": "NUMBER" }
                            },
                            "required": ["overall"]
                        }
                    },
                    "required": ["arc", "score"]
                }
            }
        },
        "required": ["selection", "reasoning", "candidates"]
    })
}

/// `schema`, a Gemini schema, as JSON Schema (Vzestup's `convertSchema`): types in lower case,
/// every object closed, optional properties left optional.
pub fn json_schema(schema: &Value) -> Value {
    let kind = schema.get("type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "OBJECT" => {
            let properties: Map<String, Value> = schema
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.clone(), json_schema(v)))
                .collect();
            let mut out = json!({ "type": "object", "properties": properties });
            if let Some(required) = schema.get("required") {
                out["required"] = required.clone();
            }
            out["additionalProperties"] = json!(false);
            out
        }
        "ARRAY" => json!({ "type": "array", "items": json_schema(&schema["items"]) }),
        "STRING" => json!({ "type": "string" }),
        _ => json!({ "type": "number" }),
    }
}

/// `schema`, a JSON Schema [`json_schema`] made, as OpenAI's strict mode takes it: every
/// property required, an optional one also allowed to be null.
pub fn strict(schema: &Value) -> Value {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let mut names = Vec::new();
            let mut properties = Map::new();
            for (name, child) in schema.get("properties").and_then(Value::as_object).into_iter().flatten() {
                let child = strict(child);
                properties
                    .insert(name.clone(), if required.contains(&name.as_str()) { child } else { nullable(child) });
                names.push(json!(name));
            }
            json!({ "type": "object", "properties": properties, "required": names, "additionalProperties": false })
        }
        Some("array") => json!({ "type": "array", "items": strict(&schema["items"]) }),
        _ => schema.clone(),
    }
}

fn nullable(schema: Value) -> Value {
    match schema.get("type").and_then(Value::as_str) {
        Some(kind @ ("string" | "number")) => json!({ "type": [kind, "null"] }),
        _ => json!({ "anyOf": [schema, { "type": "null" }] }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slides_contract_reads_in_each_providers_form() {
        let anthropic = json_schema(&slides_gemini());
        let slide = &anthropic["properties"]["slides"]["items"];
        assert_eq!(slide["additionalProperties"], json!(false));
        assert_eq!(slide["properties"]["keyBullets"], json!({ "type": "array", "items": { "type": "string" } }));
        assert_eq!(slide["properties"]["chartData"]["required"], json!(["categories", "series"]));
        assert_eq!(slide["required"].as_array().unwrap().len(), 12);
        // Scaena's: a word for each bullet's icon, which a slide may leave out (PLAN G.23).
        assert_eq!(slide["properties"]["icons"], json!({ "type": "array", "items": { "type": "string" } }));

        let openai = strict(&anthropic);
        let slide = &openai["properties"]["slides"]["items"];
        assert_eq!(slide["required"].as_array().unwrap().len(), 17);
        assert_eq!(slide["properties"]["icons"]["anyOf"][1], json!({ "type": "null" }));
        // Scaena's: the source a slide's facts come from, which a slide may leave out (FUNC-3).
        assert_eq!(slide["properties"]["sourceRef"], json!({ "type": ["string", "null"] }));
        assert_eq!(slide["properties"]["supportingFacts"], json!({ "type": ["string", "null"] }));
        assert_eq!(slide["properties"]["chartData"]["anyOf"][1], json!({ "type": "null" }));
        assert_eq!(slide["properties"]["slideTitle"], json!({ "type": "string" }));
        let series = &slide["properties"]["chartData"]["anyOf"][0]["properties"]["series"]["items"];
        assert_eq!(series["properties"]["name"], json!({ "type": ["string", "null"] }));
    }
}
