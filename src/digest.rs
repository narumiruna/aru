use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{AruError, Result};

pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

pub fn canonical_json_digest<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)
        .map_err(|error| AruError::msg(format!("could not canonicalize value: {error}")))?;
    let canonical = canonicalize_json(value);
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| AruError::msg(format!("could not encode canonical value: {error}")))?;
    Ok(sha256_bytes(&bytes))
}

pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)
        .map_err(|error| AruError::msg(format!("could not canonicalize value: {error}")))?;
    serde_json::to_vec(&canonicalize_json(value))
        .map_err(|error| AruError::msg(format!("could not encode canonical value: {error}")))
}

fn canonicalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonicalize_json).collect())
        }
        serde_json::Value::Object(values) => {
            let sorted: BTreeMap<_, _> = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize_json(value)))
                .collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        value => value,
    }
}

pub fn semantic_link_digest(target: &str, content_digest: &str) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"aru-link-v1\0");
    bytes.extend_from_slice(&(target.len() as u64).to_be_bytes());
    bytes.extend_from_slice(target.as_bytes());
    bytes.extend_from_slice(&(content_digest.len() as u64).to_be_bytes());
    bytes.extend_from_slice(content_digest.as_bytes());
    sha256_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_ignores_map_insertion_order() {
        let left = serde_json::json!({"b": 2, "a": 1});
        let right = serde_json::json!({"a": 1, "b": 2});
        assert_eq!(
            canonical_json_digest(&left).unwrap(),
            canonical_json_digest(&right).unwrap()
        );
    }
}
