use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;

pub struct LoadAverageResolver;

impl Resolver for LoadAverageResolver {
    fn name(&self) -> &'static str {
        "load_averages"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut loads = [0.0f64; 3];
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        if n < 1 {
            return vec![];
        }

        let mut map: IndexMap<String, FactValue> = IndexMap::new();
        map.insert("1m".into(), FactValue::Float(loads[0]));
        if n >= 2 {
            map.insert("5m".into(), FactValue::Float(loads[1]));
        }
        if n >= 3 {
            map.insert("15m".into(), FactValue::Float(loads[2]));
        }

        vec![("load_averages".into(), FactValue::Map(map))]
    }
}
