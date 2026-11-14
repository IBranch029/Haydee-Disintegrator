//! Heuristic safety guard.
//!
//! The guard is the single choke point through which every destructive
//! operation passes. It encodes three families of rules:
//!
//! * **Subtree protection** – whole directories that belong to the operating
//!   system (`C:\Windows`, `WindowsApps`, the Microsoft Shared folder, ...).
//!   Nothing below them may ever be touched.
//! * **Exact path protection** – container directories that must survive
//!   (`C:\Program Files`, the user profile, `AppData` itself, ...) while
//!   their *children* remain fair game.
//! * **Artefact classification** – shared runtimes (VC++ redistributables,
//!   DirectX, .NET, Python, ...) and Windows service binaries that must never
//!   be dissolved because other software depends on them.

use std::path::Path;

/// Outcome of a safety evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The artefact may be destroyed.
    Allow,
    /// The artefact is protected; the payload carries the reason.
    Protected(String),
}

impl Verdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Allow)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Verdict::Allow => None,
            Verdict::Protected(reason) => Some(reason),
        }
    }
}

/// Registry roots the engine is allowed to enumerate. `HKEY_CLASSES_ROOT` is
/// a merged view and is therefore read-only.
pub const WRITABLE_HIVES: &[&str] = &["HKEY_LOCAL_MACHINE", "HKEY_CURRENT_USER"];

/// Shared runtime / system component file name prefixes.
///
/// The list intentionally favours false positives over false negatives: a
/// surviving `vcruntime140_1.dll` is an inconvenience, a deleted one bricks
/// unrelated applications.
const PROTECTED_FILE_PATTERNS: &[&str] = &[
    // Visual C++ runtime family
    "msvcp", "msvcr", "msvc_", "vcruntime", "vcomp", "vccorlib", "concrt", "ucrtbase",
    "mfc", "mfc140", "atl", "msvbvm", "msvbvm60", "comctl32", "comdlg32",
    // Universal CRT / API sets
    "api-ms-win-", "api-ms-downlevel", "api-ms-core-", "ext-ms-",
    // DirectX / graphics runtimes
    "d3dcompiler", "d3d11", "d3d10", "d3d9", "dxgi", "dxcore", "d3dx", "xinput", "xaudio",
    "x3daudio", "vulkan", "opengl32", "glu32", "dinput", "dinput8",
    // Common shared libraries
    "openssl", "libcrypto", "libssl", "sqlite3", "libsqlite", "zlib", "zlibwapi",
    "icuuc", "icudt", "icuin", "curl", "boost_", "libgcc", "libstdc++", "libwinpthread",
    // Language runtimes
    "python", "dotnet", "coreclr", "clrjit", "mscor", "hostfxr", "hostpolicy", "java",
    "jvm", "node", "electron",
    // Core Windows binaries
    "ntdll", "kernel32", "kernelbase", "user32", "gdi32", "advapi32", "shell32", "ole32",
    "oleaut32", "sechost", "secur32", "crypt32", "wintrust", "imagehlp", "normaliz",
    "ws2_32", "wininet", "urlmon", "winhttp", "mpr", "netapi32", "userenv", "uxtheme",
    "dwmapi", "propsys", "clbcatq", "combase", "wintypes", "winmm", "version",
    "shlwapi", "msvcp60", "wow64", "wow64cpu", "wow64win", "wow64msc",
    // Service / session infrastructure
    "svchost", "lsass", "csrss", "wininit", "winlogon", "services", "smss", "conhost",
    "dllhost", "sihost", "taskhost", "explorer", "rundll32", "regsvr32", "msiexec",
    "cmd", "powershell", "pwsh", "wscript", "cscript", "mshta", "bootmgr", "boot",
    "ntoskrnl", "hal", "win32k", "clfs", "storport", "partmgr",
];

/// Exact Windows service names that must never be deleted.
const PROTECTED_SERVICES: &[&str] = &[
    "TrustedInstaller", "WinDefend", "WdNisSvc", "WdNisDrv", "WdFilter", "SecurityHealthService",
    "CryptSvc", "EventLog", "Schedule", "BITS", "wuauserv", "UsoSvc", "WaaSMedicSvc",
    "DPS", "RpcSs", "DcomLaunch", "Dhcp", "Dnscache", "LanmanWorkstation", "LanmanServer",
    "Winmgmt", "ProfSvc", "UserManager", "Themes", "AudioEndpointBuilder", "Audiosrv",
    "RemoteRegistry", "RemoteDesktopServices", "TermService", "WinHttpAutoProxySvc",
    "NlaSvc", "nsi", "Power", "Spooler", "Fax", "WSearch", "MsiServer", "InstallService",
    "StateRepository", "AppXSvc", "ClipSVC", "LicenseManager", "TokenBroker",
    "BrokerInfrastructure", "CoreMessagingRegistrar", "DoSvc", "DiagTrack", "SENS", "Lsa",
    "Sam", "Security", "SoftwareProtection", "SystemEventsBroker", "TimeBroker", "Wcmsvc",
    "WpnService", "W32Time", "wmiApSrv", "PcaSvc", "pla", "lmhosts", "SnapOrchestrator",
    "StorSvc", "TabletInputService", "Troubleshooter", "uhssvc", "UnistoreSvc", "VaultSvc",
    "XblAuthManager", "XblGameSave", "XboxNetApiSvc", "XboxGipSvc", "lldpService",
    "NcbService", "FDResPub", "fdPHost", "gpsvc", "IKEEXT", "iphlpsvc", "mpssvc",
    "Netman", "netprofm", "RasMan", "RemoteAccess", "Routing", "SessionEnv", "SharedAccess",
    "ShellHWDetection", "Tram", "UXSvc", "WpnUserService",
];

/// Registry keys that may never be deleted as a whole. Sub-keys of these
/// entries are still fair game (an `Uninstall\{App}` key, for instance).
const PROTECTED_REGISTRY_KEYS: &[(&str, &str)] = &[
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows NT"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders"),
    (
        "HKEY_LOCAL_MACHINE",
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders",
    ),
    (
        "HKEY_LOCAL_MACHINE",
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Component Based Servicing",
    ),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\Cryptography"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Microsoft\\SystemCertificates"),
    ("HKEY_LOCAL_MACHINE", "SOFTWARE\\Classes"),
    ("HKEY_LOCAL_MACHINE", "SYSTEM\\CurrentControlSet\\Control"),
    ("HKEY_LOCAL_MACHINE", "SYSTEM\\CurrentControlSet\\Enum"),
    ("HKEY_LOCAL_MACHINE", "SYSTEM\\CurrentControlSet\\Services"),
    ("HKEY_LOCAL_MACHINE", "SYSTEM\\Select"),
    ("HKEY_LOCAL_MACHINE", "SYSTEM\\Setup"),
    ("HKEY_LOCAL_MACHINE", "SECURITY"),
    ("HKEY_LOCAL_MACHINE", "SAM"),
    ("HKEY_CURRENT_USER", "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ("HKEY_CURRENT_USER", "Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ("HKEY_CURRENT_USER", "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
    ("HKEY_CURRENT_USER", "Software\\Microsoft\\Windows\\CurrentVersion\\Policies"),
    (
        "HKEY_CURRENT_USER",
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders",
    ),
    (
        "HKEY_CURRENT_USER",
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders",
    ),
    ("HKEY_CURRENT_USER", "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings"),
    ("HKEY_CURRENT_USER", "Software\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate"),
    ("HKEY_CURRENT_USER", "Software\\Classes"),
];

/// Runtime safety policy resolved against the live machine layout.
#[derive(Debug, Clone)]
pub struct SafetyGuard {
    /// Roots where *everything* below is untouchable.
    deny_subtrees: Vec<String>,
    /// Containers that must survive while their children stay disposable.
    deny_exact: Vec<String>,
    /// Lower-cased `SystemRoot` used to detect OS owned service binaries.
    system_root: String,
}

impl Default for SafetyGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyGuard {
    /// Builds the policy from the real machine layout.
    pub fn new() -> Self {
        let system_root = env_or("SystemRoot", r"C:\Windows");
        let program_files = env_or("ProgramFiles", r"C:\Program Files");
        let program_files_x86 = env_or("ProgramFiles(x86)", r"C:\Program Files (x86)");
        let program_data = env_or("ProgramData", r"C:\ProgramData");
        let user_profile = env_or("USERPROFILE", r"C:\Users\Default");

        let deny_subtrees = vec![
            system_root.clone(),
            join_path(&program_files, "WindowsApps"),
            join_path(&program_files, r"Common Files\Microsoft Shared"),
            join_path(&program_files, r"Common Files\Package Cache"),
            join_path(&program_files, r"Common Files\Windows Kits"),
            join_path(&program_files, "dotnet"),
            join_path(&program_files_x86, "dotnet"),
            join_path(&program_files_x86, r"Common Files\Microsoft Shared"),
            join_path(&program_data, r"Microsoft\Windows Defender"),
            join_path(&program_data, r"Microsoft\Windows Search"),
            join_path(&program_data, r"Microsoft\Windows Defender Advanced Threat Protection"),
            r"C:\$Recycle.Bin".to_string(),
            r"C:\System Volume Information".to_string(),
            r"C:\Recovery".to_string(),
            r"C:\EFI".to_string(),
            r"C:\Boot".to_string(),
            r"C:\PerfLogs".to_string(),
            r"C:\Documents and Settings".to_string(),
        ];

        let deny_exact = vec![
            program_files.clone(),
            program_files_x86.clone(),
            program_data.clone(),
            user_profile.clone(),
            join_path(&user_profile, "Documents"),
            join_path(&user_profile, "Desktop"),
            join_path(&user_profile, "Downloads"),
            join_path(&user_profile, "Pictures"),
            join_path(&user_profile, "Videos"),
            join_path(&user_profile, "Music"),
            join_path(&user_profile, "Saved Games"),
            join_path(&user_profile, "AppData"),
            join_path(&user_profile, r"AppData\Local"),
            join_path(&user_profile, r"AppData\Roaming"),
            join_path(&user_profile, r"AppData\LocalLow"),
            r"C:\Users\Public".to_string(),
            join_path(&program_data, r"Microsoft\Windows\Start Menu"),
            join_path(&program_data, r"Microsoft\Windows\Start Menu\Programs"),
            join_path(
                &user_profile,
                r"AppData\Roaming\Microsoft\Windows\Start Menu",
            ),
            join_path(
                &user_profile,
                r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs",
            ),
        ];

        let lower = |items: Vec<String>| {
            items
                .into_iter()
                .map(|p| p.to_ascii_lowercase())
                .collect::<Vec<_>>()
        };

        Self {
            deny_subtrees: lower(deny_subtrees),
            deny_exact: lower(deny_exact),
            system_root: system_root.to_ascii_lowercase(),
        }
    }

    /// Evaluates a filesystem path against the policy.
    pub fn check_path(&self, path: &Path) -> Verdict {
        let normalized = normalize_path(path);
        let lower = normalized.to_ascii_lowercase();

        // Shared runtime components are protected wherever they live.
        let is_directory = std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
        if !is_directory {
            if let Some(name) = file_name_of(&lower) {
                if is_protected_file_name(name) {
                    return Verdict::Protected(format!(
                        "shared runtime or system component '{name}'"
                    ));
                }
            }
        }

        // Container directories that must survive.
        for exact in &self.deny_exact {
            if lower == *exact {
                return Verdict::Protected(format!(
                    "protected system container '{}'",
                    display_path(exact)
                ));
            }
        }

        // Everything below an untouchable root.
        for root in &self.deny_subtrees {
            if lower == *root || is_ancestor(root, &lower) {
                return Verdict::Protected(format!(
                    "inside protected system subtree '{}'",
                    display_path(root)
                ));
            }
        }

        // Deleting an ancestor would take the protected artefact with it.
        for exact in &self.deny_exact {
            if is_ancestor(&lower, exact) {
                return Verdict::Protected(format!(
                    "path is an ancestor of protected location '{}'",
                    display_path(exact)
                ));
            }
        }
        for root in &self.deny_subtrees {
            if is_ancestor(&lower, root) {
                return Verdict::Protected(format!(
                    "path is an ancestor of protected subtree '{}'",
                    display_path(root)
                ));
            }
        }

        Verdict::Allow
    }

    /// Convenience wrapper returning a boolean verdict.
    pub fn path_allowed(&self, path: &Path) -> bool {
        self.check_path(path).is_allowed()
    }
}

impl SafetyGuard {
    /// Evaluates a Windows service against the policy.
    ///
    /// `image_path` may carry command line arguments (`svchost.exe -k netsvcs`);
    /// only the executable portion is considered.
    pub fn check_service(&self, name: &str, image_path: Option<&str>) -> Verdict {
        for protected in PROTECTED_SERVICES {
            if protected.eq_ignore_ascii_case(name) {
                return Verdict::Protected(format!("critical system service '{name}'"));
            }
        }

        if name.to_ascii_lowercase().starts_with("microsoft") {
            return Verdict::Protected(format!("Microsoft owned service '{name}'"));
        }

        if let Some(image) = image_path {
            let executable = image
                .trim()
                .trim_matches('"')
                .split(" -")
                .next()
                .unwrap_or(image)
                .trim_matches('"');
            if executable.is_empty() {
                return Verdict::Allow;
            }
            let normalized = normalize_path(Path::new(executable)).to_ascii_lowercase();
            if self.system_root.is_empty() {
                return Verdict::Allow;
            }
            if normalized == self.system_root || is_ancestor(&self.system_root, &normalized) {
                return Verdict::Protected(format!(
                    "service binary '{executable}' is owned by the operating system"
                ));
            }
        }

        Verdict::Allow
    }

    /// Evaluates a registry key path (`SOFTWARE\Vendor\Product`) against the
    /// policy for the given hive.
    pub fn check_registry_key(&self, hive: &str, subkey: &str) -> Verdict {
        let lower = normalize_registry_path(subkey).to_ascii_lowercase();

        for (protected_hive, protected_key) in PROTECTED_REGISTRY_KEYS {
            if protected_hive.eq_ignore_ascii_case(hive)
                && lower == protected_key.to_ascii_lowercase()
            {
                return Verdict::Protected(format!(
                    "protected registry key '{hive}\\{protected_key}'"
                ));
            }
        }

        for (protected_hive, protected_key) in PROTECTED_REGISTRY_KEYS {
            if protected_hive.eq_ignore_ascii_case(hive)
                && is_ancestor(&lower, &protected_key.to_ascii_lowercase())
            {
                return Verdict::Protected(format!(
                    "registry key is an ancestor of protected key '{hive}\\{protected_key}'"
                ));
            }
        }

        Verdict::Allow
    }

    /// Evaluates the removal of a single registry value.
    ///
    /// Values inside protected containers may still be scrubbed - that is
    /// exactly how an autostart hook of the purged target is removed - but a
    /// key that is an ancestor of a protected key stays untouched.
    pub fn check_registry_value(&self, hive: &str, subkey: &str) -> Verdict {
        match self.check_registry_key(hive, subkey) {
            Verdict::Allow => Verdict::Allow,
            Verdict::Protected(reason) => {
                if reason.contains("ancestor") {
                    Verdict::Protected(reason)
                } else {
                    Verdict::Allow
                }
            }
        }
    }
}

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// Joins two path fragments with a single separator.
fn join_path(base: &str, child: &str) -> String {
    format!(
        "{}\\{}",
        base.trim_end_matches('\\'),
        child.trim_start_matches('\\')
    )
}

/// Normalises a filesystem path: canonical separators, no `.`/`..`, no
/// trailing separator. The drive prefix is preserved verbatim.
pub fn normalize_path(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('/', "\\");
    if raw.is_empty() {
        return String::new();
    }

    let (prefix, rest) = match raw.find(':') {
        Some(index) => (raw[..=index].to_string(), &raw[index + 1..]),
        None => (String::new(), raw.as_str()),
    };

    let mut components: Vec<&str> = Vec::new();
    for component in rest.split('\\') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            other => components.push(other),
        }
    }

    let joined = components.join("\\");
    if prefix.is_empty() {
        joined
    } else if joined.is_empty() {
        prefix
    } else {
        format!("{}\\{}", prefix, joined)
    }
}

/// Normalises a registry path: single separators, no empty components.
pub fn normalize_registry_path(path: &str) -> String {
    path.replace('/', "\\")
        .split('\\')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>()
        .join("\\")
}

/// True when `ancestor` is a strict path prefix of `descendant`.
pub fn is_ancestor(ancestor: &str, descendant: &str) -> bool {
    if ancestor.is_empty() {
        return false;
    }
    let ancestor = ancestor.trim_end_matches('\\');
    ancestor.len() < descendant.len()
        && descendant[..ancestor.len()].eq_ignore_ascii_case(ancestor)
        && descendant.as_bytes()[ancestor.len()] == b'\\'
}

/// Extracts the final component of a normalised path.
pub fn file_name_of(normalized: &str) -> Option<&str> {
    normalized.rsplit('\\').next().filter(|name| !name.is_empty())
}

/// Renders a lower-cased policy path for the user interface.
fn display_path(lower: &str) -> String {
    lower.to_string()
}

/// True when the file name matches a protected shared-runtime pattern.
pub fn is_protected_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    PROTECTED_FILE_PATTERNS
        .iter()
        .any(|pattern| lower.starts_with(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> SafetyGuard {
        SafetyGuard::new()
    }

    #[test]
    fn rejects_system32_and_windows_tree() {
        let guard = guard();
        for target in [
            r"C:\Windows\System32",
            r"C:\Windows\System32\drivers",
            r"C:\Windows\explorer.exe",
            r"C:\Windows",
        ] {
            let verdict = guard.check_path(Path::new(target));
            assert!(
                !verdict.is_allowed(),
                "expected '{target}' to be protected, got {verdict:?}"
            );
        }
    }

    #[test]
    fn rejects_syswow64_and_windows_apps() {
        let guard = guard();
        let targets = [
            r"C:\Windows\SysWOW64",
            r"C:\Program Files\WindowsApps\Contoso.App_1.0.0.0_x64",
        ];
        for target in targets {
            assert!(
                !guard.check_path(Path::new(target)).is_allowed(),
                "expected '{target}' to be protected"
            );
        }
    }

    #[test]
    fn rejects_ancestors_of_protected_locations() {
        let guard = guard();
        for target in [r"C:\", r"C:\Users"] {
            assert!(
                !guard.check_path(Path::new(target)).is_allowed(),
                "expected ancestor '{target}' to be protected"
            );
        }
    }

    #[test]
    fn allows_application_payload_locations() {
        let guard = guard();
        let profile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\test".into());
        let targets = [
            r"C:\Program Files\Contoso Suite",
            r"C:\Program Files (x86)\Contoso Legacy",
            r"C:\ProgramData\Contoso Suite",
        ];
        for target in targets {
            assert!(
                guard.check_path(Path::new(target)).is_allowed(),
                "expected '{target}' to be disposable"
            );
        }
        let app_data = format!("{}\\AppData\\Local\\Contoso Suite", profile);
        assert!(guard.check_path(Path::new(&app_data)).is_allowed());
    }

    #[test]
    fn protects_shared_runtime_components() {
        for name in [
            "vcruntime140.dll",
            "vcruntime140_1.dll",
            "msvcp140.dll",
            "d3dcompiler_47.dll",
            "ucrtbase.dll",
            "api-ms-win-core-runtime-l1-1-0.dll",
            "python312.dll",
        ] {
            assert!(
                is_protected_file_name(name),
                "expected '{name}' to be classified as shared runtime"
            );
        }
        assert!(!is_protected_file_name("contoso-agent.exe"));
    }

    #[test]
    fn protects_critical_services() {
        let guard = guard();
        assert!(!guard.check_service("Dhcp", None).is_allowed());
        assert!(!guard.check_service("TrustedInstaller", None).is_allowed());
        assert!(!guard
            .check_service("winmgmt", Some(r#"C:\Windows\System32\svchost.exe -k netsvcs"#))
            .is_allowed());
        assert!(guard
            .check_service("ContosoUpdater", Some(r#"C:\Program Files\Contoso\updater.exe"#))
            .is_allowed());
    }

    #[test]
    fn protects_critical_registry_keys_but_allows_children() {
        let guard = guard();
        assert!(!guard
            .check_registry_key("HKEY_LOCAL_MACHINE", r"SOFTWARE\Microsoft\Windows NT")
            .is_allowed());
        assert!(!guard
            .check_registry_key("HKEY_LOCAL_MACHINE", r"SOFTWARE")
            .is_allowed());
        assert!(!guard
            .check_registry_key("HKEY_LOCAL_MACHINE", r"SOFTWARE\Microsoft")
            .is_allowed());
        assert!(guard
            .check_registry_key(
                "HKEY_LOCAL_MACHINE",
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Contoso Suite"
            )
            .is_allowed());
        assert!(guard
            .check_registry_key("HKEY_LOCAL_MACHINE", r"SOFTWARE\Contoso\Suite")
            .is_allowed());
    }

    #[test]
    fn normalises_paths_and_registry_keys() {
        assert_eq!(
            normalize_path(Path::new(r"C:\Program Files\..\Windows")),
            r"C:\Windows"
        );
        assert_eq!(normalize_path(Path::new(r"C:\a\b\.\")), r"C:\a\b");
        assert_eq!(
            normalize_registry_path(r"SOFTWARE\\Vendor\\.\Product"),
            r"SOFTWARE\Vendor\Product"
        );
    }

    #[test]
    fn ancestor_detection_is_strict() {
        assert!(is_ancestor(r"C:\a", r"C:\a\b"));
        assert!(!is_ancestor(r"C:\a", r"C:\a"));
        assert!(!is_ancestor(r"C:\ab", r"C:\a\b"));
    }
}

