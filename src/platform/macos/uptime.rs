use std::ffi::CString;

pub fn read_uptime_seconds() -> Option<u64> {
    let name = CString::new("kern.boottime").ok()?;
    let mut boottime: libc::timeval = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::timeval>();
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut boottime as *mut _ as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let uptime = now - boottime.tv_sec;
    if uptime < 0 { None } else { Some(uptime as u64) }
}
