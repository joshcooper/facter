use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;

/// All possible fact value types, mirroring facter's scalar/array/map hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FactValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<FactValue>),
    Map(IndexMap<String, FactValue>),
}

impl FactValue {
    /// Write using facter's default "hash" output format.
    ///
    /// Rules:
    ///   - indent == 0 (top-level): strings printed bare, no quotes
    ///   - indent  > 0 (inside map/array): strings quoted
    ///   - numbers and booleans never quoted
    ///   - maps: `{\n  key => value\n}`
    ///   - arrays: `[val1, val2]`
    pub fn write_hash(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        match self {
            FactValue::String(s) => {
                if indent == 0 {
                    write!(f, "{}", s)
                } else {
                    write!(f, "\"{}\"", s)
                }
            }
            FactValue::Integer(i) => write!(f, "{}", i),
            FactValue::Float(fl) => write!(f, "{}", fl),
            FactValue::Boolean(b) => write!(f, "{}", b),
            FactValue::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", HashDisplay { value: v, indent: indent + 1 })?;
                }
                write!(f, "]")
            }
            FactValue::Map(map) => {
                writeln!(f, "{{")?;
                let pad = "  ".repeat(indent + 1);
                let close = "  ".repeat(indent);
                for (k, v) in map {
                    write!(f, "{}{} => ", pad, k)?;
                    write!(f, "{}", HashDisplay { value: v, indent: indent + 1 })?;
                    writeln!(f)?;
                }
                write!(f, "{}}}", close)
            }
        }
    }
}

/// Wrapper to use FactValue::write_hash via the Display trait.
pub struct HashDisplay<'a> {
    pub value: &'a FactValue,
    pub indent: usize,
}

impl<'a> fmt::Display for HashDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.write_hash(f, self.indent)
    }
}
