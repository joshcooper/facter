use crate::facts::value::FactValue;
use crate::resolvers::Resolver;

pub struct PathResolver;

impl Resolver for PathResolver {
    fn name(&self) -> &'static str {
        "path"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        match std::env::var("PATH") {
            Ok(p) => vec![("path".into(), FactValue::String(p))],
            Err(_) => vec![],
        }
    }
}
