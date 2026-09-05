//! Bound YAML expansion during deserialization, including aliases, not afterward.
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_yaml_ng::{Mapping, Value};

use crate::error::{AruError, Result};
use crate::skill::SKILL_MD_MAX_BYTES;

pub(super) fn parse(text: &str) -> Result<Value> {
    if text.len() as u64 > SKILL_MD_MAX_BYTES {
        return Err(AruError::msg("skill metadata exceeds byte limit"));
    }
    BoundedValue {
        depth: 0,
        budget: &mut Budget { nodes: 0, bytes: 0 },
    }
    .deserialize(serde_yaml_ng::Deserializer::from_str(text))
    .map_err(|error| AruError::msg(format!("invalid SKILL.md frontmatter: {error}")))
}

struct Budget {
    nodes: usize,
    bytes: usize,
}

struct BoundedValue<'a> {
    depth: usize,
    budget: &'a mut Budget,
}

impl BoundedValue<'_> {
    fn string<E: de::Error>(self, value: &str) -> std::result::Result<Value, E> {
        self.budget.bytes += value.len();
        if self.budget.bytes as u64 > SKILL_MD_MAX_BYTES {
            return Err(de::Error::custom(
                "skill metadata exceeds expanded YAML byte limit",
            ));
        }
        Ok(Value::String(value.into()))
    }
}

impl<'de> DeserializeSeed<'de> for BoundedValue<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Value, D::Error> {
        self.budget.nodes += 1;
        if self.depth > 32 || self.budget.nodes > 20_000 {
            return Err(de::Error::custom(
                "skill metadata exceeds YAML structure limits",
            ));
        }
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for BoundedValue<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded YAML without tags or merge keys")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Value, E> {
        self.string(value)
    }

    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Value, E> {
        self.string(&value)
    }

    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(BoundedValue {
            depth: self.depth + 1,
            budget: self.budget,
        })? {
            values.push(value);
        }
        Ok(Value::Sequence(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        let mut values = Mapping::new();
        while let Some(key) = map.next_key_seed(BoundedValue {
            depth: self.depth + 1,
            budget: self.budget,
        })? {
            if key.as_str() == Some("<<") {
                return Err(de::Error::custom(
                    "YAML merge keys are unsupported in skill metadata",
                ));
            }
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate YAML key in skill metadata"));
            }
            let value = map.next_value_seed(BoundedValue {
                depth: self.depth + 1,
                budget: self.budget,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Mapping(values))
    }
}
