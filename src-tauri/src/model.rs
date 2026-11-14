//! Data contract shared between the Rust engine and the React console.
//!
//! Every structure in this module is serialised directly over the Tauri IPC
//! boundary, therefore all fields are plain data (no handles, no lifetimes).

use serde::{Deserialize, Serialize};

/// Discovery origin of a target application.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppSource {
    /// Classic installer registered under an `Uninstall` registry hive.
    Registry,
    /// MSIX / AppX package installed through the Microsoft Store.
    Appx,
}

impl AppSource {
    pub fn label(&self) -> &'static str {
        match self {
            AppSource::Registry => "REGISTRY",
            AppSource::Appx => "APPX",
        }
    }
}

/// A single entry of the installed-application inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    /// Stable identifier: the registry sub-key path or the MSIX package family.
    pub id: String,
    pub display_name: String,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    /// ISO-8601 date when available, otherwise the raw installer value.
    pub install_date: Option<String>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub estimated_size_bytes: Option<u64>,
    pub system_component: bool,
    pub windows_installer: bool,
    pub parent_key_name: Option<String>,
    pub source: AppSource,
    pub registry_key: Option<String>,
    /// Main executable of the target when it could be resolved.
    pub executable: Option<String>,
    /// `x64`, `x86`, `arm64` or `unknown`.
    pub architecture: String,
    /// Lower-cased match tokens used by the residue scanner.
    pub tokens: Vec<String>,
}

/// Qualitative residue severity used for the dashboard threat column.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ThreatLevel {
    Minimal,
    Low,
    Elevated,
    High,
    Critical,
}

impl ThreatLevel {
    pub fn label(&self) -> &'static str {
        match self {
            ThreatLevel::Minimal => "MINIMAL",
            ThreatLevel::Low => "LOW",
            ThreatLevel::Elevated => "ELEVATED",
            ThreatLevel::High => "HIGH",
            ThreatLevel::Critical => "CRITICAL",
        }
    }

    /// Numeric weight used to rank the dashboard.
    pub fn weight(&self) -> u32 {
        match self {
            ThreatLevel::Minimal => 0,
            ThreatLevel::Low => 25,
            ThreatLevel::Elevated => 50,
            ThreatLevel::High => 75,
            ThreatLevel::Critical => 100,
        }
    }

    /// Maps a 0..=100 score onto a threat band.
    pub fn from_score(score: u32) -> ThreatLevel {
        match score {
            0..=9 => ThreatLevel::Minimal,
            10..=34 => ThreatLevel::Low,
            35..=59 => ThreatLevel::Elevated,
            60..=89 => ThreatLevel::High,
            _ => ThreatLevel::Critical,
        }
    }
}

/// Category of a detected residue artefact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResidueKind {
    File,
    Directory,
    RegistryKey,
    RegistryValue,
    Service,
    ScheduledTask,
    StartupEntry,
    Process,
    ShellExtension,
}

impl ResidueKind {
    pub fn label(&self) -> &'static str {
        match self {
            ResidueKind::File => "FILE",
            ResidueKind::Directory => "DIRECTORY",
            ResidueKind::RegistryKey => "REGISTRY KEY",
            ResidueKind::RegistryValue => "REGISTRY VALUE",
            ResidueKind::Service => "SERVICE",
            ResidueKind::ScheduledTask => "SCHEDULED TASK",
            ResidueKind::StartupEntry => "STARTUP HOOK",
            ResidueKind::Process => "PROCESS",
            ResidueKind::ShellExtension => "SHELL EXTENSION",
        }
    }
}

/// One detected piece of residue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidueItem {
    pub id: String,
    pub kind: ResidueKind,
    /// Absolute path, registry path or service/task identifier.
    pub location: String,
    pub detail: String,
    pub size_bytes: u64,
    /// True when the safety guard refused to touch this artefact.
    pub protected: bool,
    pub protection_reason: Option<String>,
}

/// Result of the deep inspection phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidueReport {
    pub app: InstalledApp,
    pub threat: ThreatLevel,
    pub score: u32,
    pub items: Vec<ResidueItem>,
    pub total_bytes: u64,
    pub protected_items: u32,
    pub warnings: Vec<String>,
}

impl ResidueReport {
    pub fn new(app: InstalledApp) -> Self {
        Self {
            app,
            threat: ThreatLevel::Minimal,
            score: 0,
            items: Vec::new(),
            total_bytes: 0,
            protected_items: 0,
            warnings: Vec::new(),
        }
    }

    /// Recomputes the threat band from the collected residue.
    pub fn finalise(&mut self) {
        let mut score: u32 = 0;

        let services = self.count(ResidueKind::Service);
        let tasks = self.count(ResidueKind::ScheduledTask);
        let startup = self.count(ResidueKind::StartupEntry);
        let processes = self.count(ResidueKind::Process);
        let registry = self.count(ResidueKind::RegistryKey) + self.count(ResidueKind::RegistryValue);
        let files = self.count(ResidueKind::File) + self.count(ResidueKind::Directory);

        // Persistent hooks are the most dangerous residue: they survive a
        // classic uninstall and re-inject the target on every boot.
        score += services.saturating_mul(14);
        score += tasks.saturating_mul(12);
        score += startup.saturating_mul(9);
        score += processes.saturating_mul(6);
        score += registry.saturating_mul(4);
        score += files.saturating_mul(2);

        if self.total_bytes > 512 * 1024 * 1024 {
            score += 12;
        } else if self.total_bytes > 64 * 1024 * 1024 {
            score += 7;
        } else if self.total_bytes > 8 * 1024 * 1024 {
            score += 4;
        }

        self.score = score.min(100);
        self.threat = ThreatLevel::from_score(self.score);
    }

    fn count(&self, kind: ResidueKind) -> u32 {
        self.items.iter().filter(|i| i.kind == kind).count() as u32
    }
}

/// Options controlling a purge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeOptions {
    /// Execute the vendor supplied uninstaller before dissolving residue.
    pub run_official_uninstaller: bool,
    /// Create a system restore point before the first destructive action.
    pub create_restore_point: bool,
    /// Terminate running processes and their child trees.
    pub kill_processes: bool,
    /// Stop and delete Windows services owned by the target.
    pub remove_services: bool,
    /// Delete scheduled tasks owned by the target.
    pub remove_tasks: bool,
    /// Remove autostart hooks (registry `Run` values, startup shortcuts).
    pub remove_startup: bool,
    /// Schedule stubborn files for deletion after the next reboot.
    pub defer_to_reboot: bool,
    /// Remove MSIX packages through the package manager.
    pub remove_appx: bool,
}

impl Default for PurgeOptions {
    fn default() -> Self {
        Self {
            run_official_uninstaller: true,
            create_restore_point: true,
            kill_processes: true,
            remove_services: true,
            remove_tasks: true,
            remove_startup: true,
            defer_to_reboot: true,
            remove_appx: true,
        }
    }
}

/// Coarse stage of the purge pipeline, streamed to the console.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PurgeStage {
    Preflight,
    RestorePoint,
    Terminate,
    Uninstall,
    Services,
    Tasks,
    Startup,
    Files,
    Registry,
    Verify,
    Complete,
    Aborted,
}

impl PurgeStage {
    pub fn label(&self) -> &'static str {
        match self {
            PurgeStage::Preflight => "PREFLIGHT",
            PurgeStage::RestorePoint => "RESTORE POINT",
            PurgeStage::Terminate => "PROCESS NEUTRALISATION",
            PurgeStage::Uninstall => "VENDOR UNINSTALLER",
            PurgeStage::Services => "SERVICE SCRUB",
            PurgeStage::Tasks => "TASK SCHEDULER SCRUB",
            PurgeStage::Startup => "AUTOSTART SCRUB",
            PurgeStage::Files => "FILESYSTEM OBLITERATION",
            PurgeStage::Registry => "REGISTRY HUNTER",
            PurgeStage::Verify => "VERIFICATION",
            PurgeStage::Complete => "COMPLETE",
            PurgeStage::Aborted => "ABORTED",
        }
    }
}

/// Severity of a streamed event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    Info,
    Warning,
    Critical,
    Success,
}

/// Realtime event streamed from the engine to the console.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeEvent {
    pub stage: PurgeStage,
    pub severity: EventSeverity,
    pub message: String,
    /// Bytes reclaimed by the step that produced the event.
    pub bytes_delta: u64,
    /// Cumulative bytes reclaimed so far.
    pub bytes_total: u64,
    /// Artefacts destroyed so far.
    pub items_removed: u32,
    /// Global progress in the 0..=1 range.
    pub progress: f32,
}

/// Terminal summary of a purge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeReport {
    pub app_id: String,
    pub display_name: String,
    pub bytes_reclaimed: u64,
    pub files_removed: u32,
    pub files_deferred: u32,
    pub registry_keys_removed: u32,
    pub registry_values_removed: u32,
    pub services_removed: u32,
    pub tasks_removed: u32,
    pub startup_removed: u32,
    pub processes_terminated: u32,
    pub protected_items: u32,
    pub reboot_required: bool,
    pub restore_point_created: bool,
    pub duration_ms: u64,
    pub failures: Vec<crate::error::Failure>,
    pub warnings: Vec<String>,
}

/// Host telemetry sampled for the dashboard gauges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTelemetry {
    pub cpu_percent: f32,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub memory_percent: f32,
    pub disk_total_bytes: u64,
    pub disk_free_bytes: u64,
    pub process_count: u32,
    pub uptime_seconds: u64,
}

/// Static information about the host and the privilege level of the process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub build: String,
    pub architecture: String,
    pub is_elevated: bool,
    pub user_name: String,
    pub machine_name: String,
    pub webview2_version: Option<String>,
    pub app_version: String,
}

/// Metadata of a system restore point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePointInfo {
    pub sequence_number: u32,
    pub description: String,
    pub created: bool,
    pub detail: String,
}

