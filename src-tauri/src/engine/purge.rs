//! Purge execution pipeline.
//!
//! The pipeline is strictly ordered: every destructive step depends on the
//! previous one having completed (processes before files, services before
//! their registry keys, ...). Each step streams an event so that the console
//! can render the disintegration in realtime.

use std::os::windows::process::CommandExt;
use std::time::Instant;

use crate::engine::{apps, filesystem, processes, registry, residue, restore, services, startup, tasks};
use crate::engine::safety::SafetyGuard;
use crate::error::Failure;
use crate::model::{
    AppSource, EventSeverity, InstalledApp, PurgeEvent, PurgeOptions, PurgeReport, PurgeStage,
};

/// Callback used to stream realtime events to the console.
pub type EventSink<'a> = &'a mut dyn FnMut(PurgeEvent);

/// Helper that keeps the running totals in sync with the emitted events.
struct Progress {
    bytes: u64,
    items: u32,
}

impl Progress {
    fn new() -> Self {
        Self { bytes: 0, items: 0 }
    }

    fn event(
        &mut self,
        stage: PurgeStage,
        severity: EventSeverity,
        message: impl Into<String>,
        progress: f32,
    ) -> PurgeEvent {
        PurgeEvent {
            stage,
            severity,
            message: message.into(),
            bytes_delta: 0,
            bytes_total: self.bytes,
            items_removed: self.items,
            progress: progress.clamp(0.0, 1.0),
        }
    }
}

/// Runs a vendor supplied uninstaller with a hard timeout.
fn run_vendor_uninstaller(command: &str, timeout_secs: u64) -> Result<(), String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("empty uninstall command".to_string());
    }

    let mut child = std::process::Command::new("cmd")
        .args(["/C", trimmed])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|error| format!("cannot launch uninstaller: {error}"))?;

    let deadline = Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("uninstaller exited with status {status}"))
                }
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err("uninstaller timed out".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            Err(error) => return Err(format!("cannot monitor uninstaller: {error}")),
        }
    }
}

/// Executes the complete purge pipeline.
pub fn purge(app: &InstalledApp, options: &PurgeOptions, emit: EventSink<'_>) -> PurgeReport {
    let started = Instant::now();
    let guard = SafetyGuard::new();
    let mut progress = Progress::new();

    let mut report = PurgeReport {
        app_id: app.id.clone(),
        display_name: app.display_name.clone(),
        bytes_reclaimed: 0,
        files_removed: 0,
        files_deferred: 0,
        registry_keys_removed: 0,
        registry_values_removed: 0,
        services_removed: 0,
        tasks_removed: 0,
        startup_removed: 0,
        processes_terminated: 0,
        protected_items: 0,
        reboot_required: false,
        restore_point_created: false,
        duration_ms: 0,
        failures: Vec::new(),
        warnings: Vec::new(),
    };

    emit(progress.event(
        PurgeStage::Preflight,
        EventSeverity::Info,
        format!("Engaging purge protocol on '{}'", app.display_name),
        0.02,
    ));

    // --- Restore point ------------------------------------------------------
    if options.create_restore_point {
        let info = restore::create_restore_point(&format!(
            "DISINTEGRATOR purge of {}",
            app.display_name
        ));
        report.restore_point_created = info.created;
        if info.created {
            emit(progress.event(
                PurgeStage::RestorePoint,
                EventSeverity::Success,
                format!("Restore point checkpoint established ({})", info.detail),
                0.06,
            ));
        } else {
            report.warnings.push(info.detail.clone());
            emit(progress.event(
                PurgeStage::RestorePoint,
                EventSeverity::Warning,
                format!("Restore point unavailable: {}", info.detail),
                0.06,
            ));
        }
    }

    // --- Process neutralisation ---------------------------------------------
    if options.kill_processes {
        let table = processes::enumerate_processes();
        let tree = processes::build_tree(&table);
        let targets = processes::find_target_processes(app, &table);

        if targets.is_empty() {
            emit(progress.event(
                PurgeStage::Terminate,
                EventSeverity::Info,
                "No running instance of the target detected",
                0.12,
            ));
        } else {
            for process in &targets {
                let terminated = processes::terminate_process_tree(process.pid, &tree);
                progress.items += terminated;
                report.processes_terminated += terminated;
                emit(progress.event(
                    PurgeStage::Terminate,
                    EventSeverity::Critical,
                    format!(
                        "Terminated '{}' (pid {}) and {} dependent process(es)",
                        process.name, process.pid, terminated
                    ),
                    0.12,
                ));
            }
        }
    }

    // --- Vendor uninstaller -------------------------------------------------
    if options.run_official_uninstaller {
        let command = app
            .quiet_uninstall_string
            .clone()
            .or_else(|| app.uninstall_string.clone());

        match command {
            Some(command) => match run_vendor_uninstaller(&command, 180) {
                Ok(()) => emit(progress.event(
                    PurgeStage::Uninstall,
                    EventSeverity::Success,
                    "Vendor uninstaller completed successfully",
                    0.18,
                )),
                Err(error) => {
                    report.warnings.push(format!("vendor uninstaller: {error}"));
                    emit(progress.event(
                        PurgeStage::Uninstall,
                        EventSeverity::Warning,
                        format!("Vendor uninstaller reported: {error}"),
                        0.18,
                    ));
                }
            },
            None => emit(progress.event(
                PurgeStage::Uninstall,
                EventSeverity::Info,
                "Target does not expose an uninstaller - continuing with deep purge",
                0.18,
            )),
        }
    }

    // --- MSIX package -------------------------------------------------------
    if options.remove_appx && app.source == AppSource::Appx {
        let package = app.id.trim_start_matches("appx::").to_string();
        match apps::remove_appx_package(&package) {
            Ok(()) => {
                progress.items += 1;
                emit(progress.event(
                    PurgeStage::Uninstall,
                    EventSeverity::Success,
                    format!("MSIX package '{package}' deregistered"),
                    0.2,
                ));
            }
            Err(error) => {
                report.failures.push(Failure {
                    category: "appx".to_string(),
                    target: package,
                    reason: error.clone(),
                    deferred: false,
                });
                emit(progress.event(
                    PurgeStage::Uninstall,
                    EventSeverity::Warning,
                    format!("MSIX package could not be removed: {error}"),
                    0.2,
                ));
            }
        }
    }

    // --- Services and drivers ----------------------------------------------
    if options.remove_services {
        let needles = filesystem::name_variants(app);
        let catalogue = services::list_services();
        let mut targets: Vec<String> = Vec::new();

        for service in &catalogue {
            let haystack = format!(
                "{} {} {}",
                service.name,
                service.display_name.clone().unwrap_or_default(),
                service.image_path.clone().unwrap_or_default()
            )
            .to_ascii_lowercase();

            let matches = needles
                .iter()
                .any(|needle| haystack.contains(&needle.to_ascii_lowercase()))
                || app
                    .install_location
                    .as_ref()
                    .map(|root| haystack.contains(&root.to_ascii_lowercase()))
                    .unwrap_or(false);

            if matches {
                targets.push(service.name.clone());
            }
        }

        if targets.is_empty() {
            emit(progress.event(
                PurgeStage::Services,
                EventSeverity::Info,
                "No Windows service belongs to the target",
                0.32,
            ));
        } else {
            let (removed, failures) = services::purge_services(&targets, &guard);
            progress.items += removed;
            report.services_removed += removed;
            report.failures.extend(failures.clone());

            for failure in &failures {
                emit(progress.event(
                    PurgeStage::Services,
                    EventSeverity::Warning,
                    format!("Service '{}' protected: {}", failure.target, failure.reason),
                    0.32,
                ));
            }

            emit(progress.event(
                PurgeStage::Services,
                EventSeverity::Success,
                format!("Dissolved {removed} service registration(s)", removed = removed),
                0.32,
            ));
        }
    }

    // --- Scheduled tasks ----------------------------------------------------
    if options.remove_tasks {
        let needles = filesystem::name_variants(app);
        let catalogue = tasks::list_tasks();
        let (removed, failures) = tasks::purge_tasks(&catalogue, &needles);

        progress.items += removed;
        report.tasks_removed += removed;
        report.failures.extend(failures.clone());

        for failure in &failures {
            emit(progress.event(
                PurgeStage::Tasks,
                EventSeverity::Warning,
                format!("Task '{}' survived: {}", failure.target, failure.reason),
                0.42,
            ));
        }

        emit(progress.event(
            PurgeStage::Tasks,
            EventSeverity::Success,
            format!("Dissolved {removed} scheduled task(s)", removed = removed),
            0.42,
        ));
    }

    // --- Autostart hooks ----------------------------------------------------
    if options.remove_startup {
        let (removed, failures) = startup::purge_startup(app, &guard);
        progress.items += removed;
        report.startup_removed += removed;
        report.failures.extend(failures.clone());

        for failure in &failures {
            emit(progress.event(
                PurgeStage::Startup,
                EventSeverity::Warning,
                format!("Autostart hook survived: {}", failure.target),
                0.48,
            ));
        }

        emit(progress.event(
            PurgeStage::Startup,
            EventSeverity::Success,
            format!("Dissolved {removed} autostart hook(s)", removed = removed),
            0.48,
        ));
    }

    // --- Filesystem obliteration --------------------------------------------
    let paths = filesystem::collect_residue_paths(app, &guard);
    let mut file_outcome = filesystem::PurgeOutcome::default();

    for item in &paths {
        if item.protected {
            file_outcome.protected_items += 1;
            continue;
        }
        filesystem::delete_path(
            std::path::Path::new(&item.location),
            &guard,
            options.defer_to_reboot,
            &mut file_outcome,
        );
    }

    report.files_removed += file_outcome.removed;
    report.files_deferred += file_outcome.deferred;
    report.protected_items += file_outcome.protected_items;
    report.reboot_required |= file_outcome.reboot_required;
    report.failures.extend(file_outcome.failures.clone());
    progress.bytes += file_outcome.bytes;
    progress.items += file_outcome.removed;

    emit(progress.event(
        PurgeStage::Files,
        EventSeverity::Success,
        format!(
            "Obliterated {} filesystem object(s), reclaimed {} bytes",
            file_outcome.removed, file_outcome.bytes
        ),
        0.72,
    ));

    if file_outcome.deferred > 0 {
        emit(progress.event(
            PurgeStage::Files,
            EventSeverity::Warning,
            format!(
                "{} locked object(s) scheduled for deletion after the next reboot",
                file_outcome.deferred
            ),
            0.72,
        ));
    }

    // --- Registry hunter ----------------------------------------------------
    let scan = registry::scan_residue(app, &guard);
    let mut keys_removed = 0u32;
    let mut values_removed = 0u32;

    // The installer registration itself.
    if let Some(registry_key) = &app.registry_key {
        if let Some((hive, subkey)) = registry_key.split_once('\\') {
            match registry::delete_key_tree(hive, subkey, &guard) {
                Ok(()) => {
                    keys_removed += 1;
                    progress.items += 1;
                }
                Err(error) => report.failures.push(Failure {
                    category: "registry".to_string(),
                    target: registry_key.clone(),
                    reason: error,
                    deferred: false,
                }),
            }
        }
    }

    // Scattered registry trees.
    for item in &scan.items {
        if item.protected || item.kind != crate::model::ResidueKind::RegistryKey {
            continue;
        }
        let Some((hive, subkey)) = item.location.split_once('\\') else {
            continue;
        };
        match registry::delete_key_tree(hive, subkey, &guard) {
            Ok(()) => {
                keys_removed += 1;
                progress.items += 1;
            }
            Err(error) => report.failures.push(Failure {
                category: "registry".to_string(),
                target: item.location.clone(),
                reason: error,
                deferred: false,
            }),
        }
    }

    // Keys that are exclusively owned by the target.
    for (hive, subkey) in &scan.tree_candidates {
        match registry::delete_key_tree(hive, subkey, &guard) {
            Ok(()) => {
                keys_removed += 1;
                progress.items += 1;
            }
            Err(error) => report.failures.push(Failure {
                category: "registry".to_string(),
                target: format!("{hive}\\{subkey}"),
                reason: error,
                deferred: false,
            }),
        }
    }

    // Individual values (autostart hooks, context menu entries, ...).
    for (hive, subkey, value_name) in &scan.value_targets {
        match registry::delete_value(hive, subkey, value_name, &guard) {
            Ok(()) => {
                values_removed += 1;
                progress.items += 1;
            }
            Err(error) => report.failures.push(Failure {
                category: "registry".to_string(),
                target: format!("{hive}\\{subkey}\\{value_name}"),
                reason: error,
                deferred: false,
            }),
        }
    }

    report.registry_keys_removed += keys_removed;
    report.registry_values_removed += values_removed;

    emit(progress.event(
        PurgeStage::Registry,
        EventSeverity::Success,
        format!(
            "Registry hunter scrubbed {keys_removed} key(s) and {values_removed} value(s)"
        ),
        0.9,
    ));

    // --- Verification -------------------------------------------------------
    let verification = residue::analyze(app);
    let survivors = verification.items.len() as u32;

    if survivors == 0 {
        emit(progress.event(
            PurgeStage::Verify,
            EventSeverity::Success,
            "Verification passed - zero residue detected",
            0.98,
        ));
    } else {
        report.warnings.push(format!(
            "{survivors} artefact(s) survived the purge and require a reboot or manual review"
        ));
        emit(progress.event(
            PurgeStage::Verify,
            EventSeverity::Warning,
            format!("{survivors} artefact(s) survived the purge"),
            0.98,
        ));
    }

    report.bytes_reclaimed = progress.bytes;
    report.duration_ms = started.elapsed().as_millis() as u64;

    emit(progress.event(
        PurgeStage::Complete,
        EventSeverity::Success,
        format!(
            "Disintegration complete - {} bytes reclaimed in {} ms",
            report.bytes_reclaimed, report.duration_ms
        ),
        1.0,
    ));

    report
}
