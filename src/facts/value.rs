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

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: &FactValue, indent: usize) -> String {
        format!("{}", HashDisplay { value: v, indent })
    }

    #[test]
    fn string_unquoted_at_top_level() {
        assert_eq!(h(&FactValue::String("hello".into()), 0), "hello");
    }

    #[test]
    fn string_quoted_inside_container() {
        assert_eq!(h(&FactValue::String("hello".into()), 1), "\"hello\"");
    }

    #[test]
    fn integer_never_quoted() {
        assert_eq!(h(&FactValue::Integer(42), 0), "42");
        assert_eq!(h(&FactValue::Integer(42), 2), "42");
    }

    #[test]
    fn float_display() {
        assert_eq!(h(&FactValue::Float(3.14), 0), "3.14");
    }

    #[test]
    fn boolean_values() {
        assert_eq!(h(&FactValue::Boolean(true), 0), "true");
        assert_eq!(h(&FactValue::Boolean(false), 1), "false");
    }

    #[test]
    fn array_inline_format() {
        let v = FactValue::Array(vec![
            FactValue::String("a".into()),
            FactValue::String("b".into()),
        ]);
        assert_eq!(h(&v, 0), "[\"a\", \"b\"]");
    }

    #[test]
    fn empty_array() {
        assert_eq!(h(&FactValue::Array(vec![]), 0), "[]");
    }

    #[test]
    fn map_braces_and_indentation() {
        let mut m = IndexMap::new();
        m.insert("key".to_string(), FactValue::String("val".into()));
        let out = h(&FactValue::Map(m), 0);
        assert!(out.starts_with('{'));
        assert!(out.trim_end().ends_with('}'));
        assert!(out.contains("  key => \"val\""));
    }

    #[test]
    fn nested_map_extra_indentation() {
        let mut inner = IndexMap::new();
        inner.insert("deep".to_string(), FactValue::Integer(1));
        let mut outer = IndexMap::new();
        outer.insert("inner".to_string(), FactValue::Map(inner));
        let out = h(&FactValue::Map(outer), 0);
        // inner map key should be indented 4 spaces (2 levels)
        assert!(out.contains("    deep => 1"));
    }

    #[test]
    fn json_roundtrip() {
        let mut m = IndexMap::new();
        m.insert("x".to_string(), FactValue::Integer(1));
        m.insert("y".to_string(), FactValue::Boolean(true));
        let v = FactValue::Map(m);
        let json = serde_json::to_string(&v).unwrap();
        let back: FactValue = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, FactValue::Map(_)));
    }

    #[test]
    fn serde_untagged_integer_not_string() {
        // serde(untagged) must pick Integer for a bare number, not String
        let v: FactValue = serde_json::from_str("42").unwrap();
        assert!(matches!(v, FactValue::Integer(42)));
    }

    #[test]
    fn serde_untagged_boolean() {
        let v: FactValue = serde_json::from_str("true").unwrap();
        assert!(matches!(v, FactValue::Boolean(true)));
    }
}
