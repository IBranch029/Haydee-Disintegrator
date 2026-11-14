//! Tauri command surface consumed by the React console.
//!
//! Every heavy operation is dispatched to a blocking worker thread so that the
//! 120 FPS console never drops a frame while the engine works.

use tauri::ipc::Channel;

use crate::engine::{apps, purge, registry, residue, restore, sysinfo};
use crate::model::{
    InstalledApp, PurgeEvent, PurgeOptions, PurgeReport, ResidueReport, RestorePointInfo,
    SystemInfo, SystemTelemetry,
};

/// Filter applied to the dashboard inventory.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ScanFilter {
    /// Free text query matched against name, publisher and version.
    #[serde(default)]
    pub query: Option<String>,
    /// `name`, `size`, `date` or `threat`.
    #[serde(default)]
    pub sort: Option<String>,
    /// Include components that Windows flags as system components.
    #[serde(default)]
    pub include_system_components: bool,
}

/// Static information about the host and the privilege level of the process.
#[tauri::command]
pub fn system_info() -> SystemInfo {
    sysinfo::system_info()
}

/// Realtime telemetry sample for the dashboard gauges.
#[tauri::command]
pub fn telemetry() -> SystemTelemetry {
    sysinfo::telemetry()
}

/// Builds the installed application inventory.
#[tauri::command]
pub async fn scan_applications(filter: Option<ScanFilter>) -> Result<Vec<InstalledApp>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut applications = apps::enumerate_applications();

        if let Some(filter) = &filter {
            if !filter.include_system_components {
                applications.retain(|app| !app.system_component);
            }

            if let Some(query) = filter.query.as_ref() {
                let needle = query.trim().to_ascii_lowercase();
                if !needle.is_empty() {
                    applications.retain(|app| {
                        app.display_name.to_ascii_lowercase().contains(&needle)
                            || app.publisher
                                .as_ref()
                                .map(|publisher| publisher.to_ascii_lowercase().contains(&needle))
                                .unwrap_or(false)
                            || app.display_version
                                .as_ref()
                                .map(|version| version.to_ascii_lowercase().contains(&needle))
                                .unwrap_or(false)
                    });
                }
            }

            match filter.sort.as_deref().unwrap_or("name") {
                "size" => applications.sort_by(|left, right| {
                    right
                        .estimated_size_bytes
                        .unwrap_or(0)
                        .cmp(&left.estimated_size_bytes.unwrap_or(0))
                }),
                "date" => applications.sort_by(|left, right| {
                    right
                        .install_date
                        .clone()
                        .unwrap_or_default()
                        .cmp(&left.install_date.clone().unwrap_or_default())
                }),
                _ => applications.sort_by(|left, right| {
                    left.display_name
                        .to_ascii_lowercase()
                        .cmp(&right.display_name.to_ascii_lowercase())
                }),
            }
        }

        applications
    })
    .await
    .map_err(|error| error.to_string())
}

/// Runs the deep inspection of a target.
#[tauri::command]
pub async fn analyze_application(app_id: String) -> Result<ResidueReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let applications = apps::enumerate_applications();
        let application = applications
            .into_iter()
            .find(|app| app.id == app_id)
            .ok_or_else(|| format!("target '{app_id}' is no longer registered"))?;

        Ok(residue::analyze(&application))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Executes the purge pipeline and streams realtime events.
#[tauri::command]
pub async fn purge_application(
    app_id: String,
    options: PurgeOptions,
    on_event: Channel<PurgeEvent>,
) -> Result<PurgeReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let applications = apps::enumerate_applications();
        let application = applications
            .into_iter()
            .find(|app| app.id == app_id)
            .ok_or_else(|| format!("target '{app_id}' is no longer registered"))?;

        let mut sink = |event: PurgeEvent| {
            let _ = on_event.send(event);
        };

        Ok(purge::purge(&application, &options, &mut sink))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Creates a system restore point.
#[tauri::command]
pub fn create_restore_point(description: Option<String>) -> RestorePointInfo {
    let description = description.unwrap_or_else(|| "DISINTEGRATOR checkpoint".to_string());
    restore::create_restore_point(&description)
}

/// Exports a purge report as a JSON document on the desktop.
#[tauri::command]
pub fn export_report(app_id: String, report: PurgeReport) -> Result<String, String> {
    let payload = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;

    let directory = std::env::var("USERPROFILE")
        .map(|profile| std::path::PathBuf::from(profile).join("Desktop"))
        .unwrap_or_else(|_| std::env::temp_dir());

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);

    let safe_id: String = app_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();

    let path = directory.join(format!("disintegrator-{safe_id}-{stamp}.json"));
    std::fs::write(&path, payload).map_err(|error| error.to_string())?;

    Ok(path.display().to_string())
}

/// Re-scans the registry for residue of a target (used by the verification
/// panel of the console).
#[tauri::command]
pub async fn rescan_registry(app_id: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let applications = apps::enumerate_applications();
        let application = applications
            .into_iter()
            .find(|app| app.id == app_id)
            .ok_or_else(|| format!("target '{app_id}' is no longer registered"))?;

        let guard = crate::engine::safety::SafetyGuard::new();
        let scan = registry::scan_residue(&application, &guard);

        Ok(scan
            .items
            .into_iter()
            .map(|item| item.location)
            .collect::<Vec<String>>())
    })
    .await
    .map_err(|error| error.to_string())?
}

