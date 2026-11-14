//! DISINTEGRATOR - premium ultra-deep uninstaller for Windows 11.
//!
//! The crate is split into three layers:
//!
//! * [`model`] – the serialisable data contract with the React console.
//! * [`engine`] – the destructive engine (registry, filesystem, processes,
//!   services, tasks, startup hooks) behind a single safety guard.
//! * [`commands`] – the Tauri command surface consumed by the frontend.

pub mod cli;
pub mod commands;
pub mod engine;
pub mod error;
pub mod model;

use error::Result;

/// Boots the desktop console.
pub fn run() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::system_info,
            commands::telemetry,
            commands::scan_applications,
            commands::analyze_application,
            commands::purge_application,
            commands::create_restore_point,
            commands::export_report,
            commands::rescan_registry,
        ])
        .run(tauri::generate_context!())
        .map_err(|error| error::Error::Other(error.to_string()))
}
