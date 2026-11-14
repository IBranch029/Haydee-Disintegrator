//! Deep inspection (Disintegration Chamber).
//!
//! Aggregates every residue source of a target into a single, ranked report:
//! scattered files, registry trees, services and drivers, scheduled tasks,
//! autostart hooks and running processes.

use crate::engine::{apps, filesystem, processes, registry, services, startup, tasks};
use crate::engine::safety::SafetyGuard;
use crate::model::{InstalledApp, ResidueItem, ResidueKind, ResidueReport};

/// Builds the needles used to recognise the target inside system artefacts.
fn needles(app: &InstalledApp) -> Vec<String> {
    let mut values = app.tokens.clone();
    if let Some(location) = &app.install_location {
        values.push(location.trim_end_matches('\\').to_ascii_lowercase());
    }
    if let Some(executable) = &app.executable {
        values.push(executable.to_ascii_lowercase());
    }
    values.retain(|value| value.len() >= 4);
    values.sort();
    values.dedup();
    values
}

/// True when the payload references the target.
fn references(haystack: &str, needles: &[String]) -> bool {
    let lowered = haystack.to_ascii_lowercase();
    needles.iter().any(|needle| lowered.contains(needle.as_str()))
}

/// Runs the complete deep inspection of a target.
pub fn analyze(app: &InstalledApp) -> ResidueReport {
    let guard = SafetyGuard::new();
    let targets = needles(app);
    let mut report = ResidueReport::new(app.clone());

    // --- Filesystem payload -------------------------------------------------
    report
        .items
        .extend(filesystem::collect_residue_paths(app, &guard));

    // --- Registry -----------------------------------------------------------
    if let Some(registry_key) = &app.registry_key {
        if let Some((hive, subkey)) = registry_key.split_once('\\') {
            let verdict = guard.check_registry_key(hive, subkey);
            report.items.push(ResidueItem {
                id: format!("reg:{registry_key}"),
                kind: ResidueKind::RegistryKey,
                location: registry_key.clone(),
                detail: "installer registration".to_string(),
                size_bytes: 0,
                protected: !verdict.is_allowed(),
                protection_reason: verdict.reason().map(str::to_string),
            });
        }
    }

    let scan = registry::scan_residue(app, &guard);
    report.items.extend(scan.items);

    // --- Services and drivers ----------------------------------------------
    for service in services::list_services() {
        let haystack = format!(
            "{} {} {}",
            service.name,
            service.display_name.clone().unwrap_or_default(),
            service.image_path.clone().unwrap_or_default()
        );

        if !references(&haystack, &targets) {
            continue;
        }

        let verdict = guard.check_service(&service.name, service.image_path.as_deref());
        report.items.push(ResidueItem {
            id: format!("svc:{}", service.name),
            kind: ResidueKind::Service,
            location: service.name.clone(),
            detail: if service.is_driver {
                format!(
                    "kernel driver, image {}",
                    service.image_path.clone().unwrap_or_default()
                )
            } else {
                format!(
                    "windows service, image {}",
                    service.image_path.clone().unwrap_or_default()
                )
            },
            size_bytes: 0,
            protected: !verdict.is_allowed(),
            protection_reason: verdict.reason().map(str::to_string),
        });
    }

    // --- Scheduled tasks ----------------------------------------------------
    for task in tasks::list_tasks() {
        let haystack = format!("{} {}", task.task_name, task.actions.join(" "));
        if !references(&haystack, &targets) {
            continue;
        }

        report.items.push(ResidueItem {
            id: format!("task:{}", task.identifier()),
            kind: ResidueKind::ScheduledTask,
            location: task.identifier(),
            detail: if task.actions.is_empty() {
                "scheduled task".to_string()
            } else {
                format!("action: {}", task.actions.join(" | "))
            },
            size_bytes: 0,
            protected: false,
            protection_reason: None,
        });
    }

    // --- Autostart hooks ----------------------------------------------------
    for entry in startup::list_startup_entries() {
        let haystack = format!(
            "{} {}",
            entry.name,
            entry.command.clone().unwrap_or_default()
        );
        if !references(&haystack, &targets) {
            continue;
        }

        report.items.push(ResidueItem {
            id: format!("startup:{}", entry.location),
            kind: ResidueKind::StartupEntry,
            location: format!("{}\\{}", entry.location, entry.name),
            detail: entry.command.clone().unwrap_or_else(|| "autostart hook".to_string()),
            size_bytes: 0,
            protected: false,
            protection_reason: None,
        });
    }

    // --- Running processes --------------------------------------------------
    let process_table = processes::enumerate_processes();
    for process in processes::find_target_processes(app, &process_table) {
        report.items.push(ResidueItem {
            id: format!("proc:{}", process.pid),
            kind: ResidueKind::Process,
            location: format!("{} (pid {})", process.name, process.pid),
            detail: process.executable.clone().unwrap_or_else(|| "running".to_string()),
            size_bytes: 0,
            protected: false,
            protection_reason: None,
        });
    }

    report.total_bytes = report.items.iter().map(|item| item.size_bytes).sum();
    report.protected_items = report.items.iter().filter(|item| item.protected).count() as u32;

    if app.system_component {
        report
            .warnings
            .push("target is flagged as a Windows system component".to_string());
    }
    if !crate::engine::restore::system_restore_available() {
        report
            .warnings
            .push("system protection is disabled - no restore point can be created".to_string());
    }

    report.finalise();
    report
}

/// Convenience helper used by the CLI: resolves a target by id or display name.
pub fn resolve_target(query: &str) -> Option<InstalledApp> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }

    let inventory = apps::enumerate_applications();

    inventory
        .iter()
        .find(|app| {
            app.id.to_ascii_lowercase() == needle
                || app.display_name.to_ascii_lowercase() == needle
        })
        .cloned()
        .or_else(|| {
            inventory.into_iter().find(|app| {
                app.display_name.to_ascii_lowercase().contains(&needle)
                    || app.publisher
                        .as_ref()
                        .map(|publisher| publisher.to_ascii_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyses_a_known_target() {
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
            executable: None,
            architecture: "x64".into(),
            tokens: Vec::new(),
        };

        let report = analyze(&app);
        assert_eq!(report.app.display_name, "Contoso Suite");
        assert!(report.protected_items <= report.items.len() as u32);
        assert!(report.threat <= crate::model::ThreatLevel::Critical);
    }

    #[test]
    fn resolves_targets_from_the_inventory() {
        let inventory = apps::enumerate_applications();
        if let Some(first) = inventory.first() {
            let resolved = resolve_target(&first.display_name);
            assert!(resolved.is_some(), "the first target must resolve");
        }
        assert!(resolve_target("").is_none());
    }
}
