//! Host telemetry and privilege inspection.
//!
//! Powers the dashboard gauges (CPU, memory, disk) and the elevation badge.

use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;

use crate::model::{SystemInfo, SystemTelemetry};

/// CPU sampling state kept between two telemetry samples.
#[derive(Debug, Clone, Copy, Default)]
struct CpuSample {
    idle: u64,
    kernel: u64,
    user: u64,
}

fn cpu_state() -> &'static Mutex<Option<CpuSample>> {
    static STATE: OnceLock<Mutex<Option<CpuSample>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

/// Samples the current CPU utilisation.
pub fn cpu_percent() -> f32 {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    if ok == 0 {
        return 0.0;
    }

    let to_u64 = |value: FILETIME| -> u64 {
        ((value.dwHighDateTime as u64) << 32) | (value.dwLowDateTime as u64)
    };

    let sample = CpuSample {
        idle: to_u64(idle),
        kernel: to_u64(kernel),
        user: to_u64(user),
    };
    let mut state = match cpu_state().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };

    let previous = state.replace(sample);
    drop(state);

    let Some(previous) = previous else {
        return 0.0;
    };

    let idle_delta = sample.idle.saturating_sub(previous.idle);
    let total_delta = sample
        .kernel
        .saturating_sub(previous.kernel)
        .saturating_add(sample.user.saturating_sub(previous.user));

    if total_delta == 0 {
        return 0.0;
    }

    let busy = total_delta.saturating_sub(idle_delta);
    (busy as f64 / total_delta as f64 * 100.0).clamp(0.0, 100.0) as f32
}

/// Samples the physical memory usage.
pub fn memory_usage() -> (u64, u64) {
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;

    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return (0, 0);
    }

    (status.ullTotalPhys, status.ullAvailPhys)
}

/// Samples the free space of the system volume.
pub fn disk_usage() -> (u64, u64) {
    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;

    let path: Vec<u16> = "C:\\\0".encode_utf16().collect();
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut free_to_caller,
            &mut total,
            &mut total_free,
        )
    };

    if ok == 0 {
        return (0, 0);
    }

    (total, free_to_caller)
}

/// True when the process runs with administrator privileges.
pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

/// Collects the full telemetry sample.
pub fn telemetry() -> SystemTelemetry {
    let (memory_total, memory_available) = memory_usage();
    let memory_used = memory_total.saturating_sub(memory_available);
    let memory_percent = if memory_total == 0 {
        0.0
    } else {
        (memory_used as f64 / memory_total as f64 * 100.0) as f32
    };

    let (disk_total, disk_free) = disk_usage();
    let process_count = crate::engine::processes::enumerate_processes().len() as u32;

    SystemTelemetry {
        cpu_percent: cpu_percent(),
        memory_total_bytes: memory_total,
        memory_used_bytes: memory_used,
        memory_percent,
        disk_total_bytes: disk_total,
        disk_free_bytes: disk_free,
        process_count,
        uptime_seconds: unsafe { GetTickCount64() } / 1000,
    }
}

/// Reads the Windows version information from the registry.
fn windows_version() -> (String, String, String) {
    let mut product_name = "Windows".to_string();
    let mut display_version = String::new();
    let mut build = String::new();

    let Some(root) = crate::engine::registry::hive_key("HKEY_LOCAL_MACHINE") else {
        return (product_name, display_version, build);
    };

    let Ok(key) = root.open_subkey_with_flags(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        winreg::enums::KEY_READ,
    ) else {
        return (product_name, display_version, build);
    };

    if let Some(value) = crate::engine::registry::read_string(&key, "ProductName") {
        product_name = value;
    }
    if let Some(value) = crate::engine::registry::read_string(&key, "DisplayVersion") {
        display_version = value;
    }
    if let Some(value) = crate::engine::registry::read_string(&key, "CurrentBuild") {
        build = value;
    }

    (product_name, display_version, build)
}

/// Detects the installed WebView2 runtime version.
fn webview2_version() -> Option<String> {
    const CLIENT_ID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    for path in [
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients",
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients",
    ] {
        let Some(root) = crate::engine::registry::hive_key("HKEY_LOCAL_MACHINE") else {
            continue;
        };
        let Ok(key) = root.open_subkey_with_flags(
            &format!("{path}\\{CLIENT_ID}"),
            winreg::enums::KEY_READ,
        ) else {
            continue;
        };
        if let Some(version) = crate::engine::registry::read_string(&key, "pv") {
            return Some(version);
        }
    }

    None
}

/// Collects the static information about the host.
pub fn system_info() -> SystemInfo {
    let (os_name, os_version, build) = windows_version();

    let user_name = std::env::var("USERNAME").unwrap_or_else(|_| "unknown".to_string());
    let machine_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string());

    SystemInfo {
        os_name,
        os_version,
        build,
        architecture: std::env::consts::ARCH.to_string(),
        is_elevated: is_elevated(),
        user_name,
        machine_name,
        webview2_version: webview2_version(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_memory_and_disk() {
        let (total, available) = memory_usage();
        assert!(total > 0, "physical memory must be detected");
        assert!(available <= total);

        let (disk_total, disk_free) = disk_usage();
        assert!(disk_total > 0, "system volume must be detected");
        assert!(disk_free <= disk_total);
    }

    #[test]
    fn reports_system_information() {
        let info = system_info();
        assert!(!info.os_name.is_empty());
        assert!(!info.architecture.is_empty());
    }
}
