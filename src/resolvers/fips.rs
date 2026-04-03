use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use std::fs;

pub struct FipsResolver;

impl Resolver for FipsResolver {
    fn name(&self) -> &'static str {
        "fips"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let enabled = fs::read_to_string("/proc/sys/crypto/fips_enabled")
            .map(|s| s.trim() != "0")
            .unwrap_or(false);
        vec![("fips_enabled".into(), FactValue::Boolean(enabled))]
    }
}
