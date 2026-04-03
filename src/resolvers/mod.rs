pub mod identity;
pub mod kernel;
pub mod memory;
pub mod networking;
pub mod os;
pub mod processors;
pub mod timezone;
pub mod uptime;
pub mod virtualization;

use crate::facts::value::FactValue;

pub trait Resolver: Send + Sync {
    fn name(&self) -> &'static str;
    fn resolve(&self) -> Vec<(String, FactValue)>;
}

pub fn default_resolvers() -> Vec<Box<dyn Resolver>> {
    vec![
        Box::new(kernel::KernelResolver),
        Box::new(os::OsResolver),
        Box::new(identity::IdentityResolver),
        Box::new(uptime::UptimeResolver),
        Box::new(timezone::TimezoneResolver),
        Box::new(networking::NetworkingResolver),
        Box::new(memory::MemoryResolver),
        Box::new(processors::ProcessorsResolver),
        Box::new(virtualization::VirtualizationResolver),
    ]
}
