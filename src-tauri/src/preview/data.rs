//! JSON, YAML and TOML as one JSON value, for the preview's tree.

use serde::Deserialize;
use serde_json::Value;

/// Parse `text` as `format` ("json", "yaml" or "toml") into a JSON value. A YAML file with several
/// documents becomes a list of them.
pub fn structured(text: &str, format: &str) -> Result<Value, String> {
    match format {
        "json" => serde_json::from_str(text).map_err(|e| e.to_string()),
        "yaml" => {
            let mut docs = Vec::new();
            for doc in serde_yaml_ng::Deserializer::from_str(text) {
                let v = serde_yaml_ng::Value::deserialize(doc).map_err(|e| e.to_string())?;
                docs.push(yaml_to_json(v));
            }
            Ok(if docs.len() == 1 { docs.pop().expect("one document") } else { Value::Array(docs) })
        }
        "toml" => text.parse::<toml::Table>().map(|t| toml_to_json(toml::Value::Table(t))).map_err(|e| e.to_string()),
        _ => Err(format!("Unknown format {format}.")),
    }
}

fn yaml_to_json(v: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => n
            .as_i64()
            .map(Value::from)
            .or_else(|| n.as_u64().map(Value::from))
            .or_else(|| n.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number))
            .unwrap_or_else(|| Value::String(n.to_string())),
        Y::String(s) => Value::String(s),
        Y::Sequence(items) => Value::Array(items.into_iter().map(yaml_to_json).collect()),
        // Keys that aren't strings (numbers, booleans) are written as YAML would show them.
        Y::Mapping(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        Y::String(s) => s,
                        other => serde_yaml_ng::to_string(&other).map(|s| s.trim_end().to_string()).unwrap_or_default(),
                    };
                    (key, yaml_to_json(v))
                })
                .collect(),
        ),
        Y::Tagged(t) => yaml_to_json(t.value),
    }
}

fn toml_to_json(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::from(i),
        toml::Value::Float(f) => serde_json::Number::from_f64(f).map(Value::Number).unwrap_or_else(|| Value::String(f.to_string())),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Array(items.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => Value::Object(t.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn yaml_and_toml() {
        assert_eq!(structured("name: app\nports: [80, 443]\n1: one\n", "yaml").unwrap(), json!({ "name": "app", "ports": [80, 443], "1": "one" }));
        assert_eq!(structured("a: 1\n---\nb: 2\n", "yaml").unwrap(), json!([{ "a": 1 }, { "b": 2 }]));
        assert_eq!(
            structured("[package]\nname = \"x\"\nwhen = 1979-05-27\n", "toml").unwrap(),
            json!({ "package": { "name": "x", "when": "1979-05-27" } })
        );
        assert!(structured("a = ", "toml").is_err());
        // Keys keep the file's order.
        let keys: Vec<String> = structured("z: 1
a: 2
m: 3
", "yaml").unwrap().as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["z", "a", "m"]);
        assert_eq!(structured("[1, {\"a\": null}]", "json").unwrap(), json!([1, { "a": null }]));
    }
}
