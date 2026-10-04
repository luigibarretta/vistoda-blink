//! Tolerant field readers shared by the Blink inventory parsers.

use serde_json::Value;

pub fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

pub fn owned_text(value: &Value, key: &str) -> Option<String> {
    text(value, key).map(ToOwned::to_owned)
}

pub fn boolean(value: &Value, key: &str) -> Option<bool> {
    value.get(key)?.as_bool()
}

pub fn text_or_number(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .map(ToOwned::to_owned)
            .or_else(|| item.as_u64().map(|number| number.to_string()))
    })
}
