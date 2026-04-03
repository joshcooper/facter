pub mod cloud;
pub mod disk;
pub mod dmi;
pub mod filesystem;
pub mod fips;
pub mod identity;
pub mod kernel;
pub mod load_average;
pub mod memory;
pub mod networking;
pub mod os;
pub mod path;
pub mod processors;
pub mod ssh;
pub mod timezone;
pub mod uptime;
pub mod virtualization;
pub mod xen;

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
        Box::new(dmi::DmiResolver),
        Box::new(disk::DiskResolver),
        Box::new(filesystem::FilesystemResolver),
        Box::new(load_average::LoadAverageResolver),
        Box::new(fips::FipsResolver),
        Box::new(xen::XenResolver),
        Box::new(path::PathResolver),
        Box::new(ssh::SshResolver),
        Box::new(cloud::CloudResolver),
    ]
}
