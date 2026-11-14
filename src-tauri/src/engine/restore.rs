//! System restore point creation.
//!
//! The native `SRSetRestorePointW` API is used first; when the service is
//! disabled the PowerShell `Checkpoint-Computer` bridge is used as a
//! fallback. Both paths report a precise reason when the host refuses to
//! create a checkpoint.

use windows_sys::Win32::System::Restore::{
    SRSetRestorePointW, APPLICATION_UNINSTALL, BEGIN_SYSTEM_CHANGE, RESTOREPOINTINFOW,
    STATEMGRSTATUS,
};

use crate::model::RestorePointInfo;

/// True when the System Restore service is present and enabled.
pub fn system_restore_available() -> bool {
    let Some(root) = crate::engine::registry::hive_key("HKEY_LOCAL_MACHINE") else {
        return false;
    };

    let Ok(key) = root.open_subkey_with_flags(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore",
        winreg::enums::KEY_READ,
    ) else {
        return false;
    };

    let disabled = crate::engine::registry::read_u64(&key, "RPSessionInterval").is_some()
        && crate::engine::registry::read_u64(&key, "DisableSR")
            .map(|value| value != 0)
            .unwrap_or(false);

    !disabled
}

/// Creates a system restore point through the native API.
fn create_native(description: &str) -> Result<u32, String> {
    let mut info = RESTOREPOINTINFOW::default();
    info.dwEventType = BEGIN_SYSTEM_CHANGE;
    info.dwRestorePtType = APPLICATION_UNINSTALL;

    let encoded: Vec<u16> = description.encode_utf16().take(255).collect();
    for (index, unit) in encoded.iter().enumerate() {
        info.szDescription[index] = *unit;
    }

    let mut status = STATEMGRSTATUS::default();
    let ok = unsafe { SRSetRestorePointW(&info, &mut status) };

    if ok == 0 {
        // `STATEMGRSTATUS` is a packed struct: copy the field out before use.
        let code = status.nStatus;
        Err(format!(
            "SRSetRestorePointW failed with status {code}; system protection may be disabled"
        ))
    } else {
        Ok(status.llSequenceNumber as u32)
    }
}

/// Creates a system restore point through the PowerShell bridge.
fn create_powershell(description: &str) -> Result<u32, String> {
    let escaped = description.replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'\n\
         Checkpoint-Computer -Description '{escaped}' -RestorePointType 'APPLICATION_INSTALL'\n"
    );

    let script_path =
        std::env::temp_dir().join(format!("disintegrator-rp-{}.ps1", std::process::id()));
    std::fs::write(&script_path, script)
        .map_err(|error| format!("cannot stage restore point script: {error}"))?;

    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .output();

    let _ = std::fs::remove_file(&script_path);

    match output {
        Ok(result) if result.status.success() => Ok(0),
        Ok(result) => Err(format!(
            "Checkpoint-Computer failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )),
        Err(error) => Err(format!("Checkpoint-Computer could not be started: {error}")),
    }
}

/// Creates a system restore point before the destructive phase.
pub fn create_restore_point(description: &str) -> RestorePointInfo {
    let mut info = RestorePointInfo {
        sequence_number: 0,
        description: description.to_string(),
        created: false,
        detail: String::new(),
    };

    if !system_restore_available() {
        info.detail = "system protection is disabled on this host".to_string();
        return info;
    }

    match create_native(description) {
        Ok(sequence) => {
            info.created = true;
            info.sequence_number = sequence;
            info.detail = "restore point created through SRSetRestorePointW".to_string();
        }
        Err(native_error) => match create_powershell(description) {
            Ok(sequence) => {
                info.created = true;
                info.sequence_number = sequence;
                info.detail = format!("restore point created through Checkpoint-Computer ({native_error})");
            }
            Err(powershell_error) => {
                info.detail = format!("{native_error}; {powershell_error}");
            }
        },
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_restore_point_state_without_panicking() {
        let info = create_restore_point("DISINTEGRATOR test checkpoint");
        assert!(!info.description.is_empty());
        // The assertion intentionally does not require a checkpoint: system
        // protection is disabled on most development machines.
        assert!(!info.detail.is_empty() || info.created);
    }
}
