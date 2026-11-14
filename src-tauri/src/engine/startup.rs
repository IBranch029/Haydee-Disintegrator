//! Autostart hook scrub.
//!
//! Covers the registry `Run` / `RunOnce` families and the two startup
//! folders. Only entries that explicitly reference the target are removed -
//! unrelated autostart payloads are never touched.

use std::path::{Path, PathBuf};

use winreg::enums::{KEY_ALL_ACCESS, KEY_READ};

use crate::engine::safety::SafetyGuard;
use crate::error::Failure;
use crate::model::InstalledApp;

/// Registry locations that launch applications automatically.
const RUN_KEYS: &[(&str, &str)] = &[
    (
        "HKEY_CURRENT_USER",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
    ),
    (
        "HKEY_CURRENT_USER",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    ),
    (
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
    ),
    (
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    ),
    (
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
    ),
    (
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce",
    ),
];

/// An autostart entry discovered on the machine.
#[derive(Debug, Clone, Default)]
pub struct StartupEntry {
    /// Registry path or startup folder.
    pub location: String,
    /// Value name or shortcut file name.
    pub name: String,
    /// Command line or shortcut target.
    pub command: Option<String>,
    /// True when the entry is a file inside a startup folder.
    pub is_file: bool,
}

/// Returns the two startup folders of the machine.
pub fn startup_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();

    if let Ok(app_data) = std::env::var("APPDATA") {
        folders.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup"),
        );
    }

    if let Ok(program_data) = std::env::var("ProgramData") {
        folders.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup"),
        );
    }

    folders
}

/// Enumerates every autostart hook of the machine.
pub fn list_startup_entries() -> Vec<StartupEntry> {
    let mut entries = Vec::new();

    for (hive, subkey) in RUN_KEYS {
        let Some(root) = crate::engine::registry::hive_key(hive) else {
            continue;
        };
        let Ok(key) = root.open_subkey_with_flags(subkey, KEY_READ) else {
            continue;
        };

        for (name, value) in key.enum_values().flatten() {
            let command = crate::engine::registry::decode_reg_value(&value);
            entries.push(StartupEntry {
                location: format!("{}\\{}", hive.to_uppercase(), subkey),
                name,
                command,
                is_file: false,
            });
        }
    }

    for folder in startup_folders() {
        let Ok(items) = std::fs::read_dir(&folder) else {
            continue;
        };
        for item in items.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            let extension = item
                .path()
                .extension()
                .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();

            if !matches!(extension.as_str(), "lnk" | "url" | "vbs" | "cmd" | "bat" | "exe") {
                continue;
            }

            entries.push(StartupEntry {
                location: folder.display().to_string(),
                name,
                command: Some(item.path().display().to_string()),
                is_file: true,
            });
        }
    }

    entries
}

/// Removes every autostart hook that references the target.
pub fn purge_startup(app: &InstalledApp, guard: &SafetyGuard) -> (u32, Vec<Failure>) {
    let needles = crate::engine::filesystem::name_variants(app);
    let install_root = app
        .install_location
        .as_ref()
        .map(|location| location.trim_end_matches('\\').to_ascii_lowercase());

    let mut removed = 0;
    let mut failures = Vec::new();

    for entry in list_startup_entries() {
        let haystack = format!(
            "{} {}",
            entry.name,
            entry.command.clone().unwrap_or_default()
        )
        .to_ascii_lowercase();

        let references_target = needles
            .iter()
            .any(|needle| haystack.contains(&needle.to_ascii_lowercase()))
            || install_root
                .as_ref()
                .map(|root| haystack.contains(root.as_str()))
                .unwrap_or(false);

        if !references_target {
            continue;
        }

        if entry.is_file {
            let path = Path::new(&entry.location).join(&entry.name);
            let mut outcome = crate::engine::filesystem::PurgeOutcome::default();
            crate::engine::filesystem::delete_path(&path, guard, false, &mut outcome);
            if outcome.removed > 0 {
                removed += 1;
            } else if let Some(failure) = outcome.failures.into_iter().next() {
                failures.push(failure);
            }
            continue;
        }

        let Some((hive, subkey)) = split_registry_location(&entry.location) else {
            continue;
        };

        match crate::engine::registry::delete_value(&hive, &subkey, &entry.name, guard) {
            Ok(()) => removed += 1,
            Err(error) => failures.push(Failure {
                category: "startup".to_string(),
                target: format!("{}\\{}", entry.location, entry.name),
                reason: error,
                deferred: false,
            }),
        }
    }

    (removed, failures)
}

/// Splits `HKEY_LOCAL_MACHINE\SOFTWARE\...` into hive and sub-key.
fn split_registry_location(location: &str) -> Option<(String, String)> {
    let (hive, subkey) = location.split_once('\\')?;
    Some((hive.to_string(), subkey.to_string()))
}

/// Deletes a registry value with explicit access rights.
pub fn delete_run_value(hive: &str, subkey: &str, value_name: &str) -> Result<(), String> {
    let Some(root) = crate::engine::registry::hive_key(hive) else {
        return Err(format!("unsupported hive '{hive}'"));
    };
    let key = root
        .open_subkey_with_flags(subkey, KEY_ALL_ACCESS)
        .map_err(|error| format!("cannot open '{subkey}': {error}"))?;
    key.delete_value(value_name)
        .map_err(|error| format!("cannot delete value '{value_name}': {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_autostart_entries() {
        let entries = list_startup_entries();
        assert!(entries
            .iter()
            .all(|entry| !entry.location.is_empty() && !entry.name.is_empty()));
    }

    #[test]
    fn splits_registry_locations() {
        let (hive, subkey) = split_registry_location(r"HKEY_LOCAL_MACHINE\SOFTWARE\Run").unwrap();
        assert_eq!(hive, "HKEY_LOCAL_MACHINE");
        assert_eq!(subkey, r"SOFTWARE\Run");
        assert!(split_registry_location("no-separator").is_none());
    }
}
