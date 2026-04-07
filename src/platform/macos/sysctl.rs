use std::ffi::CString;

/// Query a string value from sysctl by dotted name (e.g. `"machdep.cpu.brand_string"`).
pub fn sysctl_string(name: &str) -> Option<String> {
    let name_c = CString::new(name).ok()?;
    let mut size: libc::size_t = 0;
    unsafe {
        libc::sysctlbyname(
            name_c.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        );
    }
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size];
    let ret = unsafe {
        libc::sysctlbyname(
            name_c.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }
    while buf.last() == Some(&0) {
        buf.pop();
    }
    let s = std::str::from_utf8(&buf).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Query a `u64` value from sysctl.
pub fn sysctl_u64(name: &str) -> Option<u64> {
    let name_c = CString::new(name).ok()?;
    let mut val: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    let ret = unsafe {
        libc::sysctlbyname(
            name_c.as_ptr(),
            &mut val as *mut _ as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret == 0 { Some(val) } else { None }
}

/// Query an `i32` value from sysctl.
pub fn sysctl_i32(name: &str) -> Option<i32> {
    let name_c = CString::new(name).ok()?;
    let mut val: i32 = 0;
    let mut size = std::mem::size_of::<i32>();
    let ret = unsafe {
        libc::sysctlbyname(
            name_c.as_ptr(),
            &mut val as *mut _ as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret == 0 { Some(val) } else { None }
}
