//! Canonical JSON and the event hash chain.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The hash before the first event.
pub(crate) const GENESIS: [u8; 32] = [0; 32];

/// JSON with object keys sorted (byte order) and no insignificant whitespace, so the same
/// value always gives the same bytes whatever `serde_json` features are enabled.
pub(crate) fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(k.clone()).to_string());
                out.push(':');
                if let Some(child) = map.get(k) {
                    write_canonical(child, out);
                }
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            out.push_str(&v.to_string());
        }
    }
}

/// `sha256(prev_hash || seq_be || body)`.
pub(crate) fn chain_hash(prev: &[u8; 32], seq: u64, body: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prev);
    h.update(seq.to_be_bytes());
    h.update(body.as_bytes());
    h.finalize().into()
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A stored hash column as 32 bytes.
pub(crate) fn hash32(blob: &[u8]) -> Option<[u8; 32]> {
    blob.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let v: Value = serde_json::from_str(r#"{"b":1,"a":{"d":[1,{"z":0,"y":"x"}],"c":null}}"#)
            .unwrap_or(Value::Null);
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":null,"d":[1,{"y":"x","z":0}]},"b":1}"#
        );
    }
}
