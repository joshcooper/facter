use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use indexmap::IndexMap;
use std::ffi::CStr;

pub struct IdentityResolver;

impl Resolver for IdentityResolver {
    fn name(&self) -> &'static str {
        "identity"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();

        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let euid = unsafe { libc::geteuid() };

        let username = lookup_username(uid).unwrap_or_default();
        let groupname = lookup_groupname(gid).unwrap_or_default();
        let privileged = euid == 0;

        // Legacy flat facts
        if !username.is_empty() {
            facts.push(("id".into(), FactValue::String(username.clone())));
        }
        if !groupname.is_empty() {
            facts.push(("gid".into(), FactValue::String(groupname.clone())));
        }

        // Structured identity fact
        let mut identity: IndexMap<String, FactValue> = IndexMap::new();
        identity.insert("uid".into(), FactValue::Integer(uid as i64));
        identity.insert("gid".into(), FactValue::Integer(gid as i64));
        if !username.is_empty() {
            identity.insert("user".into(), FactValue::String(username));
        }
        if !groupname.is_empty() {
            identity.insert("group".into(), FactValue::String(groupname));
        }
        identity.insert("privileged".into(), FactValue::Boolean(privileged));

        facts.push(("identity".into(), FactValue::Map(identity)));

        facts
    }
}

fn lookup_username(uid: libc::uid_t) -> Option<String> {
    let mut buf = vec![0i8; 1024];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    let ret = unsafe {
        libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result)
    };

    if ret != 0 || result.is_null() {
        return None;
    }

    unsafe {
        Some(
            CStr::from_ptr((*result).pw_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn lookup_groupname(gid: libc::gid_t) -> Option<String> {
    let mut buf = vec![0i8; 1024];
    let mut grp: libc::group = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::group = std::ptr::null_mut();

    let ret = unsafe {
        libc::getgrgid_r(gid, &mut grp, buf.as_mut_ptr(), buf.len(), &mut result)
    };

    if ret != 0 || result.is_null() {
        return None;
    }

    unsafe {
        Some(
            CStr::from_ptr((*result).gr_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}
