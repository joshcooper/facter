use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;

pub struct VirtualizationResolver;

impl Resolver for VirtualizationResolver {
    fn name(&self) -> &'static str {
        "virtualization"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let (virt, is_virtual) = crate::platform::detect_virtualization();

        facts.push(("virtual".into(), FactValue::String(virt.clone())));
        facts.push(("is_virtual".into(), FactValue::Boolean(is_virtual)));

        // Structured hypervisors fact (present only when virtual)
        if is_virtual && virt != "physical" {
            let mut hypervisors: IndexMap<String, FactValue> = IndexMap::new();
            hypervisors.insert(virt.clone(), FactValue::Map(IndexMap::new()));
            facts.push(("hypervisors".into(), FactValue::Map(hypervisors)));
        }

        facts
    }
}
