//! Registry Hunter.
//!
//! Responsibilities:
//! * enumerate the installed-application inventory from every `Uninstall` hive;
//! * deep-scan the registry for residue that references the target
//!   (shell extensions, file associations, context menu entries, ...);
//! * scrub keys and values while honouring the safety guard.

use std::collections::VecDeque;
use std::path::Path;

use winreg::enums::{
    HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_ALL_ACCESS,
    KEY_READ, KEY_WOW64_64KEY, REG_EXPAND_SZ, REG_SZ,
};
use winreg::{RegKey, RegValue};

use crate::engine::safety::SafetyGuard;
use crate::model::{ResidueItem, ResidueKind};

/// Every location that carries classic installer registrations.
pub const UNINSTALL_ROOTS: &[(&str, &str, u32)] = &[
    (
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        KEY_WOW64_64KEY,
    ),
    (
        "HKEY_LOCAL_MACHINE",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        KEY_WOW64_64KEY,
    ),
    (
        "HKEY_CURRENT_USER",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        KEY_WOW64_64KEY,
    ),
];

/// Registry roots that are traversed during the deep residue scan.
const DEEP_SCAN_ROOTS: &[(&str, &str, u32)] = &[
    ("HKEY_LOCAL_MACHINE", "SOFTWARE", KEY_WOW64_64KEY),
    ("HKEY_LOCAL_MACHINE", r"SOFTWARE\WOW6432Node", KEY_WOW64_64KEY),
    ("HKEY_CURRENT_USER", "SOFTWARE", KEY_WOW64_64KEY),
    ("HKEY_CLASSES_ROOT", "", KEY_WOW64_64KEY),
];

/// Maximum depth explored below a scan root.
const DEEP_SCAN_DEPTH: usize = 4;
/// Upper bound for visited keys **per scan root**, keeps the scan bounded on
/// huge machines without starving the later roots.
const DEEP_SCAN_KEY_BUDGET: usize = 45_000;

/// Raw registration entry of an installed application.
#[derive(Debug, Clone, Default)]
pub struct UninstallEntry {
    pub hive: String,
    pub subkey: String,
    pub display_name: String,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
    pub install_location: Option<String>,
    pub install_source: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub estimated_size_kb: Option<u64>,
    pub system_component: bool,
    pub windows_installer: bool,
    pub parent_key_name: Option<String>,
    pub display_icon: Option<String>,
    pub help_link: Option<String>,
    pub modify_path: Option<String>,
}

/// Resolves the handle of a well known hive.
pub fn hive_handle(hive: &str) -> Option<winreg::reg_key::HKEY> {
    match hive.to_ascii_uppercase().as_str() {
        "HKEY_LOCAL_MACHINE" => Some(HKEY_LOCAL_MACHINE),
        "HKEY_CURRENT_USER" => Some(HKEY_CURRENT_USER),
        "HKEY_CLASSES_ROOT" => Some(HKEY_CLASSES_ROOT),
        "HKEY_USERS" => Some(HKEY_USERS),
        _ => None,
    }
}

/// Opens a well known hive.
pub fn hive_key(hive: &str) -> Option<RegKey> {
    hive_handle(hive).map(RegKey::predef)
}

/// Decodes a raw registry value into a string when it holds text.
pub fn decode_reg_value(raw: &RegValue<'_>) -> Option<String> {
    match raw.vtype {
        REG_SZ | REG_EXPAND_SZ => {
            let utf16: Vec<u16> = raw
                .bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            let end = utf16.iter().position(|unit| *unit == 0).unwrap_or(utf16.len());
            let text = String::from_utf16_lossy(&utf16[..end]);
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        _ => None,
    }
}

/// Reads a string value, tolerating malformed UTF-16 payloads.
pub fn read_string(key: &RegKey, name: &str) -> Option<String> {
    let raw = key.get_raw_value(name).ok()?;
    decode_reg_value(&raw)
}

/// Reads a numeric value regardless of its storage width.
pub fn read_u64(key: &RegKey, name: &str) -> Option<u64> {
    if let Ok(value) = key.get_value::<u64, _>(name) {
        return Some(value);
    }
    if let Ok(value) = key.get_value::<u32, _>(name) {
        return Some(u64::from(value));
    }
    None
}

/// Reads a DWORD and interprets it as a boolean flag.
pub fn read_bool(key: &RegKey, name: &str) -> bool {
    read_u64(key, name).map(|value| value != 0).unwrap_or(false)
}

/// Converts the installer date format `YYYYMMDD` into an ISO-8601 date.
pub fn normalize_install_date(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 {
        let year = &digits[0..4];
        let month = &digits[4..6];
        let day = &digits[6..8];
        return Some(format!("{year}-{month}-{day}"));
    }
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

/// Enumerates every registration found in the well known uninstall hives.
pub fn enumerate_uninstall_entries() -> Vec<UninstallEntry> {
    let mut entries = Vec::new();

    for (hive, path, view) in UNINSTALL_ROOTS {
        let Some(root) = hive_key(hive) else { continue };
        let Ok(key) = root.open_subkey_with_flags(path, KEY_READ | *view) else {
            continue;
        };

        for name in key.enum_keys().flatten() {
            let subkey = format!("{path}\\{name}");
            let Ok(entry_key) = root.open_subkey_with_flags(&subkey, KEY_READ | *view) else {
                continue;
            };

            let display_name = match read_string(&entry_key, "DisplayName") {
                Some(name) => name,
                None => continue,
            };

            entries.push(UninstallEntry {
                hive: (*hive).to_string(),
                subkey: subkey.clone(),
                display_name,
                display_version: read_string(&entry_key, "DisplayVersion"),
                publisher: read_string(&entry_key, "Publisher"),
                install_date: read_string(&entry_key, "InstallDate")
                    .and_then(|raw| normalize_install_date(&raw)),
                install_location: read_string(&entry_key, "InstallLocation"),
                install_source: read_string(&entry_key, "InstallSource"),
                uninstall_string: read_string(&entry_key, "UninstallString"),
                quiet_uninstall_string: read_string(&entry_key, "QuietUninstallString"),
                estimated_size_kb: read_u64(&entry_key, "EstimatedSize"),
                system_component: read_bool(&entry_key, "SystemComponent"),
                windows_installer: read_bool(&entry_key, "WindowsInstaller"),
                parent_key_name: read_string(&entry_key, "ParentKeyName"),
                display_icon: read_string(&entry_key, "DisplayIcon"),
                help_link: read_string(&entry_key, "HelpLink"),
                modify_path: read_string(&entry_key, "ModifyPath"),
            });
        }
    }

    entries
}

/// Builds the set of needles used to recognise the target inside registry data.
pub fn build_match_tokens(app: &crate::model::InstalledApp) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();

    if let Some(location) = &app.install_location {
        tokens.push(location.to_ascii_lowercase());
        if let Some(folder) = Path::new(location).file_name() {
            tokens.push(folder.to_string_lossy().to_ascii_lowercase());
        }
    }
    if let Some(executable) = &app.executable {
        tokens.push(executable.to_ascii_lowercase());
        if let Some(name) = Path::new(executable).file_name() {
            tokens.push(name.to_string_lossy().to_ascii_lowercase());
        }
    }
    if let Some(publisher) = &app.publisher {
        tokens.push(publisher.to_ascii_lowercase());
    }

    let display = app.display_name.to_ascii_lowercase();
    if display.len() >= 4 {
        tokens.push(display.clone());
    }

    tokens
        .into_iter()
        .filter(|token| token.len() >= 4 && !token.is_empty())
        .collect()
}

/// Result of a deep residue scan.
#[derive(Debug, Default)]
pub struct RegistryScan {
    pub items: Vec<ResidueItem>,
    /// Individual values that reference the target: (hive, sub-key, value).
    pub value_targets: Vec<(String, String, String)>,
    /// Keys whose every textual value references the target. Such a key is
    /// owned by the target and is dissolved as a whole: (hive, sub-key).
    pub tree_candidates: Vec<(String, String)>,
}

/// Deep-scans the registry for entries that reference the target.
///
/// The walk is breadth first, depth limited and budgeted so that a hostile
/// machine cannot turn the scan into a denial of service.
pub fn scan_residue(app: &crate::model::InstalledApp, guard: &SafetyGuard) -> RegistryScan {
    let tokens = build_match_tokens(app);
    let mut scan = RegistryScan::default();

    for (hive, root_path, view) in DEEP_SCAN_ROOTS {
        let Some(handle) = hive_handle(hive) else { continue };
        let root = RegKey::predef(handle);
        let mut visited = 0usize;

        let start = if root_path.is_empty() {
            RegKey::predef(handle)
        } else {
            match root.open_subkey_with_flags(root_path, KEY_READ | *view) {
                Ok(key) => key,
                Err(_) => continue,
            }
        };

        let mut queue: VecDeque<(RegKey, String, usize)> = VecDeque::new();
        queue.push_back((start, root_path.to_string(), 0));

        while let Some((key, path, depth)) = queue.pop_front() {
            if visited >= DEEP_SCAN_KEY_BUDGET {
                break;
            }
            visited += 1;

            let key_name = path.rsplit('\\').next().unwrap_or_default().to_ascii_lowercase();
            let name_matches = tokens.iter().any(|token| key_name.contains(token.as_str()));

            // Values that point into the installation folder.
            let mut value_hits: Vec<String> = Vec::new();
            let mut text_values = 0usize;
            for value in key.enum_values().flatten() {
                let (value_name, data) = value;
                let payload = decode_reg_value(&data);
                let Some(payload) = payload else { continue };
                text_values += 1;
                let lowered = payload.to_ascii_lowercase();
                if tokens.iter().any(|token| lowered.contains(token.as_str())) {
                    value_hits.push(value_name);
                }
            }

            // A key whose every textual value points at the target belongs to
            // the target exclusively and is therefore dissolved as a whole.
            if !value_hits.is_empty() && text_values == value_hits.len() && !path.is_empty() {
                scan.tree_candidates.push((hive.to_string(), path.clone()));
            }

            if name_matches || !value_hits.is_empty() {
                let location = format!("{}\\{}", hive.to_uppercase(), path);
                let guard_verdict = if path.is_empty() {
                    crate::engine::safety::Verdict::Allow
                } else {
                    guard.check_registry_key(hive, &path)
                };

                for value_name in &value_hits {
                    scan.value_targets.push((
                        hive.to_string(),
                        path.clone(),
                        value_name.clone(),
                    ));
                }

                scan.items.push(ResidueItem {
                    id: format!("reg:{location}"),
                    kind: if value_hits.is_empty() {
                        ResidueKind::RegistryKey
                    } else {
                        ResidueKind::RegistryValue
                    },
                    location: location.clone(),
                    detail: if value_hits.is_empty() {
                        format!("key name references '{}'", app.display_name)
                    } else {
                        format!("value(s) {} reference the target", value_hits.join(", "))
                    },
                    size_bytes: 0,
                    protected: !guard_verdict.is_allowed(),
                    protection_reason: guard_verdict.reason().map(str::to_string),
                });
            }

            if depth >= DEEP_SCAN_DEPTH {
                continue;
            }

            for child in key.enum_keys().flatten() {
                let child_path = if path.is_empty() {
                    child.clone()
                } else {
                    format!("{path}\\{child}")
                };
                if let Ok(child_key) = key.open_subkey_with_flags(&child, KEY_READ | *view) {
                    queue.push_back((child_key, child_path, depth + 1));
                }
            }
        }
    }

    scan
}

/// Splits a registry path into its parent and the final component.
fn split_last(path: &str) -> (String, String) {
    match path.rfind('\\') {
        Some(index) => (path[..index].to_string(), path[index + 1..].to_string()),
        None => (String::new(), path.to_string()),
    }
}

/// True when the given key exists.
pub fn key_exists(hive: &str, subkey: &str) -> bool {
    let Some(root) = hive_key(hive) else { return false };
    root.open_subkey_with_flags(subkey, KEY_READ).is_ok()
}

/// True when the given value exists inside the key.
pub fn value_exists(hive: &str, subkey: &str, value_name: &str) -> bool {
    let Some(root) = hive_key(hive) else { return false };
    match root.open_subkey_with_flags(subkey, KEY_READ) {
        Ok(key) => key.get_raw_value(value_name).is_ok(),
        Err(_) => false,
    }
}

/// Deletes a registry key together with all of its sub-keys and values.
pub fn delete_key_tree(hive: &str, subkey: &str, guard: &SafetyGuard) -> Result<(), String> {
    let verdict = guard.check_registry_key(hive, subkey);
    if !verdict.is_allowed() {
        return Err(verdict
            .reason()
            .unwrap_or("protected registry key")
            .to_string());
    }
    if subkey.trim().is_empty() {
        return Err("refusing to delete a registry root".to_string());
    }

    let root = hive_key(hive).ok_or_else(|| format!("unsupported hive '{hive}'"))?;
    let (parent, leaf) = split_last(subkey);
    let parent_key = if parent.is_empty() {
        root
    } else {
        root.open_subkey_with_flags(&parent, KEY_ALL_ACCESS)
            .map_err(|error| format!("cannot open parent '{parent}': {error}"))?
    };

    parent_key
        .delete_subkey_all(&leaf)
        .map_err(|error| format!("cannot delete '{subkey}': {error}"))
}

/// Deletes a single registry value.
pub fn delete_value(
    hive: &str,
    subkey: &str,
    value_name: &str,
    guard: &SafetyGuard,
) -> Result<(), String> {
    let verdict = guard.check_registry_value(hive, subkey);
    if !verdict.is_allowed() {
        return Err(verdict
            .reason()
            .unwrap_or("protected registry value")
            .to_string());
    }

    let root = hive_key(hive).ok_or_else(|| format!("unsupported hive '{hive}'"))?;
    let key = root
        .open_subkey_with_flags(subkey, KEY_ALL_ACCESS)
        .map_err(|error| format!("cannot open '{subkey}': {error}"))?;
    key.delete_value(value_name)
        .map_err(|error| format!("cannot delete value '{value_name}': {error}"))
}

/// Lists the value names of a key (used to scrub individual values).
pub fn value_names(hive: &str, subkey: &str) -> Vec<String> {
    let Some(root) = hive_key(hive) else { return Vec::new() };
    match root.open_subkey_with_flags(subkey, KEY_READ) {
        Ok(key) => key.enum_values().flatten().map(|(name, _)| name).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ROOT: &str = r"Software\Disintegrator\Tests";

    fn guard() -> SafetyGuard {
        SafetyGuard::new()
    }

    #[test]
    fn normalises_installer_dates() {
        assert_eq!(
            normalize_install_date("20240115").as_deref(),
            Some("2024-01-15")
        );
        assert_eq!(normalize_install_date(""), None);
        assert_eq!(normalize_install_date("unknown").as_deref(), Some("unknown"));
    }

    #[test]
    fn extracts_match_tokens_from_application_metadata() {
        let app = crate::model::InstalledApp {
            id: "test".into(),
            display_name: "Contoso Suite".into(),
            display_version: None,
            publisher: Some("Contoso Ltd".into()),
            install_date: None,
            install_location: Some(r"C:\Program Files\Contoso Suite".into()),
            uninstall_string: None,
            quiet_uninstall_string: None,
            estimated_size_bytes: None,
            system_component: false,
            windows_installer: false,
            parent_key_name: None,
            source: crate::model::AppSource::Registry,
            registry_key: None,
            executable: Some(r"C:\Program Files\Contoso Suite\agent.exe".into()),
            architecture: "x64".into(),
            tokens: Vec::new(),
        };

        let tokens = build_match_tokens(&app);
        assert!(tokens.iter().any(|token| token.contains("contoso suite")));
        assert!(tokens.iter().any(|token| token == "agent.exe"));
    }

    #[test]
    fn creates_and_removes_registry_tree() {
        let guard = guard();
        let root = hive_key("HKEY_CURRENT_USER").expect("HKCU");
        let (key, _) = root.create_subkey(TEST_ROOT).expect("create test key");
        let (child, _) = key.create_subkey("Child").expect("create child key");
        let _ = child.set_value("Payload", &"residue");

        assert!(key_exists("HKEY_CURRENT_USER", TEST_ROOT));
        assert!(value_exists(
            "HKEY_CURRENT_USER",
            r"Software\Disintegrator\Tests\Child",
            "Payload"
        ));

        delete_key_tree("HKEY_CURRENT_USER", TEST_ROOT, &guard).expect("delete test tree");
        assert!(!key_exists("HKEY_CURRENT_USER", TEST_ROOT));
    }

    #[test]
    fn removes_single_registry_value() {
        let guard = guard();
        let root = hive_key("HKEY_CURRENT_USER").expect("HKCU");
        let (key, _) = root
            .create_subkey(r"Software\Disintegrator\Tests")
            .expect("create test key");
        let _ = key.set_value("Hook", &"C:\\Program Files\\Contoso\\agent.exe");

        delete_value(
            "HKEY_CURRENT_USER",
            r"Software\Disintegrator\Tests",
            "Hook",
            &guard,
        )
        .expect("delete value");

        assert!(!value_exists(
            "HKEY_CURRENT_USER",
            r"Software\Disintegrator\Tests",
            "Hook"
        ));
    }

    #[test]
    fn refuses_to_delete_protected_registry_keys() {
        let guard = guard();
        let error = delete_key_tree("HKEY_LOCAL_MACHINE", r"SOFTWARE\Microsoft", &guard)
            .expect_err("protected key must be rejected");
        assert!(error.contains("ancestor") || error.contains("protected"));
    }
}

