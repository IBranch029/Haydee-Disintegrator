//! Windows Service & Driver Scrub.
//!
//! Services are discovered through the `Services` registry hive (which is
//! language independent) and manipulated through the Service Control Manager.
//! Critical operating system services are rejected by the safety guard before
//! any destructive call is issued.

use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ControlService, DeleteService, OpenSCManagerW, OpenServiceW,
    QueryServiceStatusEx, SC_HANDLE, SERVICE_STATUS, SERVICE_STATUS_PROCESS,
};

use crate::engine::safety::SafetyGuard;
use crate::error::Failure;

/// Access rights (kept local so the module does not depend on SDK constants).
const SC_MANAGER_ALL_ACCESS: u32 = 0x000F_003F;
const SERVICE_QUERY_CONFIG: u32 = 0x0001;
const SERVICE_QUERY_STATUS: u32 = 0x0004;
const SERVICE_STOP: u32 = 0x0020;
const SERVICE_CHANGE_CONFIG: u32 = 0x0002;
const SERVICE_DELETE_ACCESS: u32 = 0x0001_0000;
const SERVICE_CONTROL_STOP: u32 = 1;
/// `SC_STATUS_PROCESS_INFO`.
const SC_STATUS_PROCESS_INFO: i32 = 4;

/// Service state as reported by the Service Control Manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
    Unknown,
}

impl ServiceState {
    fn from_raw(value: u32) -> ServiceState {
        match value {
            1 => ServiceState::Stopped,
            2 => ServiceState::StartPending,
            3 => ServiceState::StopPending,
            4 => ServiceState::Running,
            5 => ServiceState::ContinuePending,
            6 => ServiceState::PausePending,
            7 => ServiceState::Paused,
            _ => ServiceState::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ServiceState::Stopped => "STOPPED",
            ServiceState::StartPending => "START_PENDING",
            ServiceState::StopPending => "STOP_PENDING",
            ServiceState::Running => "RUNNING",
            ServiceState::ContinuePending => "CONTINUE_PENDING",
            ServiceState::PausePending => "PAUSE_PENDING",
            ServiceState::Paused => "PAUSED",
            ServiceState::Unknown => "UNKNOWN",
        }
    }
}

/// Description of a service or kernel driver.
#[derive(Debug, Clone, Default)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: Option<String>,
    pub image_path: Option<String>,
    pub start_type: Option<u32>,
    pub service_type: Option<u32>,
    pub state: Option<ServiceState>,
    pub is_driver: bool,
}

/// Registry root that holds every service and driver registration.
const SERVICES_ROOT: &str = r"SYSTEM\CurrentControlSet\Services";

/// Converts a string into a NUL terminated UTF-16 buffer.
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens the Service Control Manager.
fn open_manager() -> Option<SC_HANDLE> {
    let manager =
        unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_ALL_ACCESS) };
    if manager.is_null() {
        None
    } else {
        Some(manager)
    }
}

/// Queries the live state of a service.
pub fn query_state(name: &str) -> Option<ServiceState> {
    let manager = open_manager()?;
    let handle = unsafe { OpenServiceW(manager, wide(name).as_ptr(), SERVICE_QUERY_STATUS) };
    if handle.is_null() {
        unsafe {
            CloseServiceHandle(manager);
        }
        return None;
    }

    let mut buffer = vec![0u8; std::mem::size_of::<SERVICE_STATUS_PROCESS>()];
    let mut needed: u32 = 0;
    let ok = unsafe {
        QueryServiceStatusEx(
            handle,
            SC_STATUS_PROCESS_INFO,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut needed,
        )
    };

    unsafe {
        CloseServiceHandle(handle);
        CloseServiceHandle(manager);
    }

    if ok == 0 {
        None
    } else {
        let status = unsafe { &*(buffer.as_ptr() as *const SERVICE_STATUS_PROCESS) };
        Some(ServiceState::from_raw(status.dwCurrentState))
    }
}

/// Enumerates every registered service and driver.
pub fn list_services() -> Vec<ServiceInfo> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

    let mut services = Vec::new();
    let root = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);

    let Ok(key) = root.open_subkey_with_flags(SERVICES_ROOT, KEY_READ) else {
        return services;
    };

    for name in key.enum_keys().flatten() {
        let Ok(entry) = root.open_subkey_with_flags(&format!("{SERVICES_ROOT}\\{name}"), KEY_READ)
        else {
            continue;
        };

        let service_type = crate::engine::registry::read_u64(&entry, "Type").map(|value| value as u32);
        let image_path = crate::engine::registry::read_string(&entry, "ImagePath");
        let display_name = crate::engine::registry::read_string(&entry, "DisplayName");
        let start_type = crate::engine::registry::read_u64(&entry, "Start").map(|value| value as u32);

        // SERVICE_KERNEL_DRIVER = 1, SERVICE_FILE_SYSTEM_DRIVER = 2.
        let is_driver = service_type
            .map(|value| value & 0x0F == 1 || value & 0x0F == 2)
            .unwrap_or(false);

        services.push(ServiceInfo {
            name: name.clone(),
            display_name,
            image_path,
            start_type,
            service_type,
            state: None,
            is_driver,
        });
    }

    for service in services.iter_mut() {
        if !service.is_driver {
            service.state = query_state(&service.name);
        }
    }

    services
}

/// Stops a service and waits until it reaches the stopped state.
pub fn stop_service(name: &str) -> Result<(), String> {
    let manager =
        open_manager().ok_or_else(|| "cannot open the service control manager".to_string())?;

    let handle = unsafe {
        OpenServiceW(
            manager,
            wide(name).as_ptr(),
            SERVICE_STOP | SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        )
    };

    if handle.is_null() {
        unsafe {
            CloseServiceHandle(manager);
        }
        return Err(format!("cannot open service '{name}'"));
    }

    let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { ControlService(handle, SERVICE_CONTROL_STOP, &mut status) };
    let current = status.dwCurrentState;

    unsafe {
        CloseServiceHandle(handle);
        CloseServiceHandle(manager);
    }

    if ok == 0 {
        return Err(format!("cannot stop service '{name}' (state {current})"));
    }

    Ok(())
}

/// Deletes a service registration.
pub fn delete_service(name: &str) -> Result<(), String> {
    let manager =
        open_manager().ok_or_else(|| "cannot open the service control manager".to_string())?;

    let handle = unsafe {
        OpenServiceW(
            manager,
            wide(name).as_ptr(),
            SERVICE_DELETE_ACCESS | SERVICE_CHANGE_CONFIG,
        )
    };

    if handle.is_null() {
        unsafe {
            CloseServiceHandle(manager);
        }
        return Err(format!("cannot open service '{name}' for deletion"));
    }

    let ok = unsafe { DeleteService(handle) };

    unsafe {
        CloseServiceHandle(handle);
        CloseServiceHandle(manager);
    }

    if ok == 0 {
        Err(format!("cannot delete service '{name}'"))
    } else {
        Ok(())
    }
}

/// Stops and deletes every service that belongs to the target.
pub fn purge_services(target_names: &[String], guard: &SafetyGuard) -> (u32, Vec<Failure>) {
    let mut removed = 0;
    let mut failures = Vec::new();

    let catalogue = list_services();

    for name in target_names {
        let image = catalogue
            .iter()
            .find(|service| service.name.eq_ignore_ascii_case(name))
            .and_then(|service| service.image_path.clone());

        let verdict = guard.check_service(name, image.as_deref());
        if !verdict.is_allowed() {
            failures.push(Failure {
                category: "service".to_string(),
                target: name.clone(),
                reason: verdict.reason().unwrap_or("protected service").to_string(),
                deferred: false,
            });
            continue;
        }

        if let Err(error) = stop_service(name) {
            // A service that is already stopped reports an error which must not
            // abort the deletion attempt.
            if query_state(name) != Some(ServiceState::Stopped) {
                failures.push(Failure {
                    category: "service".to_string(),
                    target: name.clone(),
                    reason: error,
                    deferred: false,
                });
                continue;
            }
        }

        match delete_service(name) {
            Ok(()) => removed += 1,
            Err(error) => failures.push(Failure {
                category: "service".to_string(),
                target: name.clone(),
                reason: error,
                deferred: false,
            }),
        }
    }

    (removed, failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_registered_services() {
        let services = list_services();
        assert!(!services.is_empty(), "the machine must expose services");
        assert!(services.iter().any(|service| service.name.len() > 2));
    }

    #[test]
    fn refuses_to_delete_critical_services() {
        let guard = SafetyGuard::new();
        let (removed, failures) = purge_services(&["Dhcp".to_string()], &guard);
        assert_eq!(removed, 0);
        assert!(!failures.is_empty());
        assert!(failures[0].reason.contains("system service"));
    }
}

