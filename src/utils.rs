use automerge::ScalarValue;
use std::borrow::Cow;

/// Convert a ScalarValue to a String, handling different types appropriately.
/// This is used to ensure that when we print values from the Automerge document,
/// they are displayed in a human-readable format. (removes quotes from strings, etc.)
pub fn clean_string(value: Cow<'_, ScalarValue>) -> String {
    match value.as_ref() {
        ScalarValue::Str(s) => s.to_string(),
        ScalarValue::Int(i) => i.to_string(),
        ScalarValue::Uint(u) => u.to_string(),
        ScalarValue::F64(f) => f.to_string(),
        ScalarValue::Boolean(b) => b.to_string(),
        other => other.to_string(),
    }
}
