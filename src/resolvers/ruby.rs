use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::process::Command;

pub struct RubyResolver;

impl Resolver for RubyResolver {
    fn name(&self) -> &'static str {
        "ruby"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let script = r#"require 'rbconfig'; puts RUBY_PLATFORM; puts RbConfig::CONFIG['sitelibdir']; puts RUBY_VERSION"#;
        let output = match Command::new("ruby").arg("-e").arg(script).output() {
            Ok(o) if o.status.success() => o.stdout,
            _ => return Vec::new(),
        };
        let text = String::from_utf8_lossy(&output);
        let mut lines = text.lines();
        let platform = lines.next().unwrap_or("").to_string();
        let sitedir = lines.next().unwrap_or("").to_string();
        let version = lines.next().unwrap_or("").to_string();

        if version.is_empty() {
            return Vec::new();
        }

        let mut ruby: IndexMap<String, FactValue> = IndexMap::new();
        if !platform.is_empty() {
            ruby.insert("platform".into(), FactValue::String(platform));
        }
        if !sitedir.is_empty() {
            ruby.insert("sitedir".into(), FactValue::String(sitedir));
        }
        ruby.insert("version".into(), FactValue::String(version));

        vec![("ruby".into(), FactValue::Map(ruby))]
    }
}
