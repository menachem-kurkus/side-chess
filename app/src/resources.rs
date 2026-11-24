//! What the app itself is using, for the home screen: its memory against the budget
//! (see LIMIT_MB) and how long a redraw takes. Read with the system's own counters;
//! nothing here runs unless the home screen is on show.

/// The memory budget for the app, in MB: the figure Task Manager calls "Memory" on Windows
/// (private working set). CI (scripts/check-memory) fails above it.
pub const LIMIT_MB: f32 = 30.0;

/// How much of the budget is in use now, as the home screen's gauge shows it: 0 to 1 and over.
pub fn share(used_mb: f32) -> f32 {
    (used_mb / LIMIT_MB).max(0.0)
}

/// This process's private memory in MB, or None if the system won't say.
/// Windows: the private working set (what Task Manager shows as Memory).
/// Linux: resident pages that are not shared. macOS: resident size, from `ps`.
#[cfg(windows)]
pub fn memory_mb() -> Option<f32> {
    // PROCESS_MEMORY_COUNTERS_EX2 (Windows 10 1809 and later has the private working set)
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_non_paged: usize,
        quota_non_paged: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private_usage: usize,
        private_working_set: usize,
        shared_commit: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    // SAFETY: a plain C struct filled in by the system call, whose size we pass.
    unsafe {
        let mut c: Counters = std::mem::zeroed();
        c.cb = std::mem::size_of::<Counters>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return None;
        }
        let bytes = if c.private_working_set > 0 {
            c.private_working_set
        } else {
            c.private_usage // an older Windows does not report the private working set
        };
        Some(bytes as f32 / 1_048_576.0)
    }
}

#[cfg(target_os = "linux")]
pub fn memory_mb() -> Option<f32> {
    // /proc/self/statm: size resident shared ... (in pages)
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let mut f = text.split_whitespace().skip(1);
    let resident: f32 = f.next()?.parse().ok()?;
    let shared: f32 = f.next()?.parse().ok()?;
    Some((resident - shared).max(0.0) * 4096.0 / 1_048_576.0)
}

#[cfg(target_os = "macos")]
pub fn memory_mb() -> Option<f32> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let kb: f32 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb / 1024.0)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn memory_mb() -> Option<f32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_reports_some_memory_and_it_is_a_sane_size() {
        // Tests run in a bigger process than the app, so only check it is plausible.
        let mb = memory_mb().expect("the system reports this process's memory");
        assert!(mb > 0.5 && mb < 2000.0, "{mb}");
    }

    #[test]
    fn the_gauge_is_the_share_of_the_budget() {
        assert!((share(15.0) - 0.5).abs() < 1e-6);
        assert!(share(45.0) > 1.0, "over budget reads as more than 1");
        assert_eq!(share(-1.0), 0.0);
    }
}
