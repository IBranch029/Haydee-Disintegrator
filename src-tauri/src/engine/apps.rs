//! Installed application inventory.
//!
//! Combines classic installer registrations (registry `Uninstall` hives) with
//! MSIX / AppX packages and normalises them into a single data model.

use std::path::Path;

use crate::engine::registry::{self, UninstallEntry};
use crate::model::{AppSource, InstalledApp};

/// Extracts an executable path from a command line or icon reference.
pub fn extract_path(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(start) = trimmed.find('"') {
        if let Some(end) = trimmed[start + 1..].find('"') {
            let candidate = &trimmed[start + 1..start + 1 + end];
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }

    let lowered = trimmed.to_ascii_lowercase();
    if let Some(index) = lowered.find(".exe") {
        return Some(trimmed[..index + 4].to_string());
    }

    None
}

/// Resolves the main executable of an installer registration.
fn resolve_executable(entry: &UninstallEntry) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();

    if let Some(icon) = &entry.display_icon {
        let cleaned = icon.trim_end_matches(",0").trim();
        if let Some(path) = extract_path(cleaned) {
            candidates.push(path);
        }
    }

    if let Some(uninstall) = &entry.quiet_uninstall_string {
        if let Some(path) = extract_path(uninstall) {
            candidates.push(path);
        }
    }

    if let Some(uninstall) = &entry.uninstall_string {
        if let Some(path) = extract_path(uninstall) {
            candidates.push(path);
        }
    }

    for candidate in candidates {
        if Path::new(&candidate).is_file() {
            return Some(candidate);
        }
    }

    // Fall back to the first executable inside the installation folder.
    if let Some(location) = &entry.install_location {
        let folder = Path::new(location);
        if folder.is_dir() {
            if let Ok(items) = std::fs::read_dir(folder) {
                let mut executables: Vec<String> = items
                    .flatten()
                    .filter_map(|item| {
                        let path = item.path();
                        if path.is_file()
                            && path
                                .extension()
                                .map(|ext| ext.eq_ignore_ascii_case("exe"))
                                .unwrap_or(false)
                        {
                            Some(path.display().to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                executables.sort();
                if let Some(first) = executables.into_iter().next() {
                    return Some(first);
                }
            }
        }
    }

    None
}

/// Determines the architecture of an executable payload.
fn detect_architecture(executable: Option<&String>) -> String {
    let Some(path) = executable else {
        return "unknown".to_string();
    };

    let Ok(data) = std::fs::read(path) else {
        return "unknown".to_string();
    };

    if data.len() < 0x40 {
        return "unknown".to_string();
    }

    // PE header: the machine field lives at offset 0x3C + 4.
    let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    if pe_offset + 6 > data.len() {
        return "unknown".to_string();
    }

    let machine = u16::from_le_bytes([data[pe_offset + 4], data[pe_offset + 5]]);
    match machine {
        0x8664 => "x64".to_string(),
        0x014C => "x86".to_string(),
        0xAA64 => "arm64".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Converts an installer registration into the application model.
fn to_application(entry: &UninstallEntry) -> InstalledApp {
    let executable = resolve_executable(entry);
    let architecture = detect_architecture(executable.as_ref());

    let mut application = InstalledApp {
        id: format!("{}::{}", entry.hive, entry.subkey),
        display_name: entry.display_name.clone(),
        display_version: entry.display_version.clone(),
        publisher: entry.publisher.clone(),
        install_date: entry.install_date.clone(),
        install_location: entry.install_location.clone(),
        uninstall_string: entry.uninstall_string.clone(),
        quiet_uninstall_string: entry.quiet_uninstall_string.clone(),
        estimated_size_bytes: entry.estimated_size_kb.map(|kb| kb.saturating_mul(1024)),
        system_component: entry.system_component,
        windows_installer: entry.windows_installer,
        parent_key_name: entry.parent_key_name.clone(),
        source: AppSource::Registry,
        registry_key: Some(format!("{}\\{}", entry.hive, entry.subkey)),
        executable,
        architecture,
        tokens: Vec::new(),
    };

    application.tokens = registry::build_match_tokens(&application);
    application
}

/// Enumerates MSIX / AppX packages installed for the current user.
pub fn enumerate_appx_packages() -> Vec<InstalledApp> {
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$items = @(Get-AppxPackage | ForEach-Object {
    [PSCustomObject]@{
        Name           = [string]$_.Name
        PackageFullName = [string]$_.PackageFullName
        Version        = [string]$_.Version
        Publisher      = [string]$_.Publisher
        InstallLocation = [string]$_.InstallLocation
        Architecture   = [string]$_.Architecture
        NonRemovable   = [bool]$_.NonRemovable
        IsFramework    = [bool]$_.IsFramework
    }
})
ConvertTo-Json -InputObject $items -Depth 4 -Compress
"#;

    let script_path = std::env::temp_dir().join(format!("disintegrator-appx-{}.ps1", std::process::id()));
    if std::fs::write(&script_path, SCRIPT).is_err() {
        return Vec::new();
    }

    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .output();

    let _ = std::fs::remove_file(&script_path);

    let Ok(output) = output else {
        return Vec::new();
    };

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Vec::new();
    }

    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };

    let entries = match parsed {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(_) => vec![parsed],
        _ => Vec::new(),
    };

    entries
        .into_iter()
        .filter_map(|item| {
            let name = item.get("Name").and_then(serde_json::Value::as_str)?;
            let package = item
                .get("PackageFullName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(name);
            let version = item
                .get("Version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let publisher = item
                .get("Publisher")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let install_location = item
                .get("InstallLocation")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let architecture = item
                .get("Architecture")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            let mut application = InstalledApp {
                id: format!("appx::{package}"),
                display_name: name.to_string(),
                display_version: version,
                publisher,
                install_date: None,
                install_location,
                uninstall_string: None,
                quiet_uninstall_string: None,
                estimated_size_bytes: None,
                system_component: item
                    .get("IsFramework")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                windows_installer: false,
                parent_key_name: None,
                source: AppSource::Appx,
                registry_key: None,
                executable: None,
                architecture,
                tokens: Vec::new(),
            };

            application.tokens = registry::build_match_tokens(&application);
            Some(application)
        })
        .collect()
}

/// Removes an MSIX package from the current user.
pub fn remove_appx_package(package_full_name: &str) -> Result<(), String> {
    let escaped = package_full_name.replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference = 'Stop'\n\
         Remove-AppxPackage -Package '{escaped}' -ErrorAction Stop\n"
    );

    let script_path =
        std::env::temp_dir().join(format!("disintegrator-appxrm-{}.ps1", std::process::id()));
    std::fs::write(&script_path, script)
        .map_err(|error| format!("cannot stage removal script: {error}"))?;

    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .output();

    let _ = std::fs::remove_file(&script_path);

    match output {
        Ok(result) if result.status.success() => Ok(()),
        Ok(result) => Err(format!(
            "cannot remove package '{}': {}",
            package_full_name,
            String::from_utf8_lossy(&result.stderr).trim()
        )),
        Err(error) => Err(format!("cannot remove package '{package_full_name}': {error}")),
    }
}

/// Builds the complete inventory of the machine.
pub fn enumerate_applications() -> Vec<InstalledApp> {
    let mut applications: Vec<InstalledApp> = registry::enumerate_uninstall_entries()
        .iter()
        .map(to_application)
        .collect();

    applications.extend(enumerate_appx_packages());

    applications.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
    });

    applications
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_executables_from_command_lines() {
        assert_eq!(
            extract_path(r#""C:\Program Files\Contoso\agent.exe" --silent"#).as_deref(),
            Some(r"C:\Program Files\Contoso\agent.exe")
        );
        assert_eq!(
            extract_path(r"C:\Program Files\Contoso\agent.exe /uninstall").as_deref(),
            Some(r"C:\Program Files\Contoso\agent.exe")
        );
        assert_eq!(extract_path("no executable here"), None);
    }

    #[test]
    fn builds_a_non_empty_inventory() {
        let applications = enumerate_applications();
        assert!(
            !applications.is_empty(),
            "the machine must expose applications"
        );
        assert!(applications
            .iter()
            .all(|app| !app.display_name.trim().is_empty()));
    }
}
