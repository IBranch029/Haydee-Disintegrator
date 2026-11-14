//! Filesystem Obliteration.
//!
//! The module removes an installation payload completely:
//!
//! * reparse points (junctions, symlinks) are unlinked instead of followed;
//! * read-only and system attributes are cleared before deletion;
//! * ownership is taken over when the target is protected by an ACL;
//! * files that are still locked are scheduled for deletion after the next
//!   reboot through `MoveFileExW(MOVEFILE_DELAY_UNTIL_REBOOT)`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    GetFileAttributesW, MoveFileExW, SetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
    MOVEFILE_DELAY_UNTIL_REBOOT,
};

use crate::engine::safety::SafetyGuard;
use crate::error::Failure;
use crate::model::{InstalledApp, ResidueItem, ResidueKind};

/// Outcome of a filesystem obliteration pass.
#[derive(Debug, Default)]
pub struct PurgeOutcome {
    pub bytes: u64,
    pub removed: u32,
    pub deferred: u32,
    pub protected_items: u32,
    pub failures: Vec<Failure>,
    pub reboot_required: bool,
}

/// Converts a path into a NUL terminated UTF-16 buffer.
fn wide(path: &Path) -> Vec<u16> {
    OsStr::new(path.as_os_str())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Reads the Win32 file attributes of a path.
pub fn file_attributes(path: &Path) -> u32 {
    let buffer = wide(path);
    let attributes = unsafe { GetFileAttributesW(buffer.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        0
    } else {
        attributes
    }
}

/// True when the path is a reparse point (junction, symlink, mount point).
pub fn is_reparse_point(path: &Path) -> bool {
    file_attributes(path) & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// True when the path exists (following no links).
pub fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Clears the read-only and system attributes of a path.
pub fn clear_attributes(path: &Path) -> bool {
    let buffer = wide(path);
    unsafe { SetFileAttributesW(buffer.as_ptr(), FILE_ATTRIBUTE_NORMAL) != 0 }
}

/// Recursively computes the size of a directory tree.
///
/// Symlinks and junctions are not traversed: their payload belongs to another
/// volume and must never be counted - or deleted - as part of the target.
pub fn directory_size(path: &Path) -> u64 {
    let Ok(metadata) = path.symlink_metadata() else {
        return 0;
    };

    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return 0;
    }

    if metadata.is_file() {
        return metadata.len();
    }

    if !metadata.is_dir() {
        return 0;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };

    entries
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

/// Schedules a path for deletion during the next boot.
pub fn schedule_delete_on_reboot(path: &Path) -> bool {
    let mut source = wide(path);
    // `\\??\\` turns the path into a native object name which is required for
    // the boot time deletion of directories.
    let prefix: Vec<u16> = "\\??\\".encode_utf16().collect();
    let mut prefixed = prefix;
    prefixed.append(&mut source);

    unsafe { MoveFileExW(prefixed.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) != 0 }
}

/// Attempts to take ownership of a path so that protected payloads can be
/// removed. The operation is best effort: failures are reported by the caller.
pub fn take_ownership(path: &Path) -> bool {
    let target = path.to_string_lossy().to_string();
    let granted = std::process::Command::new("takeown.exe")
        .args(["/F", &target, "/R", "/D", "Y"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !granted {
        return false;
    }

    std::process::Command::new("icacls.exe")
        .args([&target, "/grant", "*S-1-5-32-544:F", "/T", "/C", "/Q"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Deletes a single file or an entire directory tree.
pub fn delete_path(
    path: &Path,
    guard: &SafetyGuard,
    defer: bool,
    outcome: &mut PurgeOutcome,
) {
    let verdict = guard.check_path(path);
    if !verdict.is_allowed() {
        outcome.protected_items += 1;
        outcome.failures.push(Failure {
            category: "filesystem".to_string(),
            target: path.display().to_string(),
            reason: verdict.reason().unwrap_or("protected").to_string(),
            deferred: false,
        });
        return;
    }

    if !exists(path) {
        return;
    }

    let attributes = file_attributes(path);
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY != 0;

    // Reparse points are unlinked, never traversed.
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        clear_attributes(path);
        let result = if is_directory {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Ok(()) => outcome.removed += 1,
            Err(error) => outcome.failures.push(Failure {
                category: "filesystem".to_string(),
                target: path.display().to_string(),
                reason: format!("cannot unlink reparse point: {error}"),
                deferred: false,
            }),
        }
        return;
    }

    if is_directory {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                delete_path(&entry.path(), guard, defer, outcome);
            }
        }

        clear_attributes(path);
        match std::fs::remove_dir(path) {
            Ok(()) => outcome.removed += 1,
            Err(error) => {
                if defer && schedule_delete_on_reboot(path) {
                    outcome.deferred += 1;
                    outcome.reboot_required = true;
                } else if take_ownership(path) && std::fs::remove_dir(path).is_ok() {
                    outcome.removed += 1;
                } else {
                    outcome.failures.push(Failure {
                        category: "filesystem".to_string(),
                        target: path.display().to_string(),
                        reason: format!("cannot remove directory: {error}"),
                        deferred: false,
                    });
                }
            }
        }
        return;
    }

    let size = std::fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0);
    clear_attributes(path);

    match std::fs::remove_file(path) {
        Ok(()) => {
            outcome.removed += 1;
            outcome.bytes += size;
        }
        Err(error) => {
            if defer && schedule_delete_on_reboot(path) {
                outcome.deferred += 1;
                outcome.reboot_required = true;
                outcome.bytes += size;
            } else if take_ownership(path) && std::fs::remove_file(path).is_ok() {
                outcome.removed += 1;
                outcome.bytes += size;
            } else {
                outcome.failures.push(Failure {
                    category: "filesystem".to_string(),
                    target: path.display().to_string(),
                    reason: format!("cannot remove file: {error}"),
                    deferred: false,
                });
            }
        }
    }
}

/// Obliterates a list of paths and returns the aggregated outcome.
pub fn purge_paths(
    paths: &[PathBuf],
    guard: &SafetyGuard,
    defer: bool,
) -> PurgeOutcome {
    let mut outcome = PurgeOutcome::default();
    for path in paths {
        delete_path(path, guard, defer, &mut outcome);
    }
    outcome
}

/// Builds the name variants used to locate scattered payload directories.
pub fn name_variants(app: &InstalledApp) -> Vec<String> {
    let mut variants: Vec<String> = Vec::new();

    let mut push = |value: &str| {
        let cleaned = value.trim();
        if cleaned.len() >= 3 {
            let lower = cleaned.to_ascii_lowercase();
            if !variants.contains(&lower) {
                variants.push(lower);
            }
        }
    };

    push(&app.display_name);

    let compact: String = app
        .display_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if compact.len() >= 3 {
        push(&compact);
    }

    if let Some(first) = app.display_name.split_whitespace().next() {
        if first.len() >= 4 {
            push(first);
        }
    }

    if let Some(location) = &app.install_location {
        if let Some(folder) = Path::new(location).file_name() {
            push(&folder.to_string_lossy());
        }
    }

    if let Some(executable) = &app.executable {
        if let Some(stem) = Path::new(executable).file_stem() {
            push(&stem.to_string_lossy());
        }
    }

    if let Some(publisher) = &app.publisher {
        push(publisher);
    }

    variants
}

/// Returns the base directories that may contain payload of the target.
fn base_directories() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();

    let mut push_env = |name: &str| {
        if let Ok(value) = std::env::var(name) {
            if !value.trim().is_empty() {
                bases.push(PathBuf::from(value));
            }
        }
    };

    push_env("ProgramFiles");
    push_env("ProgramFiles(x86)");
    push_env("ProgramData");
    push_env("LOCALAPPDATA");
    push_env("APPDATA");
    push_env("TEMP");
    push_env("TMP");
    push_env("SystemRoot");
    push_env("USERPROFILE");

    bases
}

/// Collects every payload location that belongs to the target.
pub fn collect_residue_paths(app: &InstalledApp, guard: &SafetyGuard) -> Vec<ResidueItem> {
    let variants = name_variants(app);
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    fn push(path: PathBuf, candidates: &mut Vec<PathBuf>, seen: &mut Vec<String>) {
        let key = path.to_string_lossy().to_ascii_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            candidates.push(path);
        }
    }

    if let Some(location) = &app.install_location {
        if !location.trim().is_empty() {
            push(PathBuf::from(location), &mut candidates, &mut seen);
        }
    }

    let bases = base_directories();

    for base in &bases {
        for variant in &variants {
            let direct = base.join(variant);
            if exists(&direct) {
                push(direct, &mut candidates, &mut seen);
            }
        }

        // Publisher scoped folders (`%LOCALAPPDATA%\Vendor\Product`).
        if let Some(publisher) = &app.publisher {
            let publisher_dir = base.join(publisher);
            if publisher_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&publisher_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                        let matches = variants.iter().any(|variant| {
                            name.starts_with(variant.as_str()) || variant.starts_with(&name)
                        });
                        if matches {
                            push(entry.path(), &mut candidates, &mut seen);
                        }
                    }
                }
            }
        }

        // Direct children whose name starts with one of the variants.
        if base.is_dir() {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    let matches = variants
                        .iter()
                        .any(|variant| name.starts_with(variant.as_str()));
                    if matches {
                        push(entry.path(), &mut candidates, &mut seen);
                    }
                }
            }
        }
    }

    // Windows specific residue: prefetch, crash dumps, start menu, logs.
    let mut extra: Vec<PathBuf> = Vec::new();

    if let Ok(system_root) = std::env::var("SystemRoot") {
        let prefetch = PathBuf::from(&system_root).join("Prefetch");
        if let Ok(entries) = std::fs::read_dir(&prefetch) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if variants.iter().any(|variant| name.starts_with(variant.as_str()))
                    && name.ends_with(".pf")
                {
                    extra.push(entry.path());
                }
            }
        }
        for log_dir in ["Logs", "Temp"] {
            let candidate = PathBuf::from(&system_root).join(log_dir);
            for variant in &variants {
                let path = candidate.join(variant);
                if exists(&path) {
                    extra.push(path);
                }
            }
        }
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let dumps = PathBuf::from(&local_app_data).join("CrashDumps");
        if let Ok(entries) = std::fs::read_dir(&dumps) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if variants.iter().any(|variant| name.starts_with(variant.as_str()))
                    && name.ends_with(".dmp")
                {
                    extra.push(entry.path());
                }
            }
        }
    }

    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let saved_games = PathBuf::from(&user_profile).join("Saved Games");
        for variant in &variants {
            let path = saved_games.join(variant);
            if exists(&path) {
                extra.push(path);
            }
        }
    }

    for start_menu in [
        std::env::var("ProgramData")
            .ok()
            .map(|data| PathBuf::from(data).join("Microsoft\\Windows\\Start Menu\\Programs")),
        std::env::var("APPDATA")
            .ok()
            .map(|data| PathBuf::from(data).join("Microsoft\\Windows\\Start Menu\\Programs")),
    ]
    .into_iter()
    .flatten()
    {
        for variant in &variants {
            let path = start_menu.join(variant);
            if exists(&path) {
                extra.push(path);
            }
        }
    }

    for path in extra {
        push(path, &mut candidates, &mut seen);
    }

    candidates
        .into_iter()
        .map(|path| {
            let metadata = std::fs::symlink_metadata(&path).ok();
            let is_directory = metadata
                .as_ref()
                .map(|meta| meta.is_dir())
                .unwrap_or(false);
            let size = if is_directory {
                directory_size(&path)
            } else {
                metadata.as_ref().map(|meta| meta.len()).unwrap_or(0)
            };
            let verdict = guard.check_path(&path);
            ResidueItem {
                id: format!("file:{}", path.display()),
                kind: if is_directory {
                    ResidueKind::Directory
                } else {
                    ResidueKind::File
                },
                location: path.display().to_string(),
                detail: if is_directory {
                    "payload directory".to_string()
                } else {
                    "payload file".to_string()
                },
                size_bytes: size,
                protected: !verdict.is_allowed(),
                protection_reason: verdict.reason().map(str::to_string),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_tree_and_reports_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("Contoso Suite");
        std::fs::create_dir_all(root.join("bin")).expect("create tree");
        std::fs::write(root.join("bin").join("agent.exe"), vec![0u8; 4096]).expect("write");
        std::fs::write(root.join("readme.txt"), vec![0u8; 16]).expect("write");

        let guard = SafetyGuard::new();
        let outcome = purge_paths(&[root.clone()], &guard, false);

        assert!(!root.exists(), "payload directory must be gone");
        assert!(outcome.removed >= 3, "expected removals, got {outcome:?}");
        assert!(outcome.bytes >= 4112, "expected bytes to be counted");
    }

    #[test]
    fn refuses_to_touch_protected_locations() {
        let guard = SafetyGuard::new();
        let mut outcome = PurgeOutcome::default();
        delete_path(
            Path::new(r"C:\Windows\System32"),
            &guard,
            false,
            &mut outcome,
        );

        assert_eq!(outcome.removed, 0);
        assert_eq!(outcome.protected_items, 1);
        assert!(!outcome.failures.is_empty());
    }

    #[test]
    fn unlinks_junctions_without_following_them() {
        let temp = tempfile::tempdir().expect("tempdir");
        let payload = temp.path().join("payload");
        let keep = temp.path().join("keep");
        std::fs::create_dir_all(&payload).expect("payload");
        std::fs::create_dir_all(&keep).expect("keep");
        std::fs::write(keep.join("important.txt"), b"keep me").expect("write");

        let junction = temp.path().join("junction");
        let created = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&payload)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        if !created {
            // Junctions cannot be created in this environment, the remaining
            // assertions are not applicable.
            return;
        }

        let guard = SafetyGuard::new();
        let mut outcome = PurgeOutcome::default();
        delete_path(&junction, &guard, false, &mut outcome);

        assert!(!junction.exists(), "junction must be unlinked");
        assert!(keep.join("important.txt").exists(), "payload must survive");
        assert!(payload.exists(), "target of the junction must survive");
    }

    #[test]
    fn plain_directories_are_not_reparse_points() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plain = temp.path().join("plain");
        std::fs::create_dir_all(&plain).expect("create");
        assert!(!is_reparse_point(&plain));
        assert!(exists(&plain));
    }

    #[test]
    fn derives_name_variants_from_metadata() {
        let app = InstalledApp {
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

        let variants = name_variants(&app);
        assert!(variants.contains(&"contoso suite".to_string()));
        assert!(variants.contains(&"contoso".to_string()));
        assert!(variants.contains(&"agent".to_string()));
    }
}

