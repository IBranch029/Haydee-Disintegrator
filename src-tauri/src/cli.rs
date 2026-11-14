//! Command line interface.
//!
//! The console can be driven headlessly, which makes DISINTEGRATOR usable
//! from deployment scripts and CI pipelines:
//!
//! ```text
//! disintegrator --scan
//! disintegrator --analyze "Contoso Suite"
//! disintegrator --purge "Contoso Suite" --no-restore-point
//! ```

use crate::engine::{apps, purge, residue};
use crate::model::{PurgeEvent, PurgeOptions, PurgeStage};

/// Re-attaches the process to the parent console.
///
/// The release build is compiled as a windows subsystem application, therefore
/// the CLI has to bind the parent console explicitly to emit its output.
pub fn attach_console() {
    unsafe {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Parsed command line action.
#[derive(Debug, Clone)]
pub enum CliAction {
    /// Launch the graphical console.
    Gui,
    /// Print the installed application inventory.
    Scan,
    /// Print the deep inspection report of a target.
    Analyze(String),
    /// Execute the purge pipeline for a target.
    Purge { query: String, options: PurgeOptions },
    /// Print the version banner.
    Version,
    /// Print the usage banner.
    Help,
}

/// Parses the process arguments.
pub fn parse(args: &[String]) -> CliAction {
    let mut action = CliAction::Gui;
    let mut options = PurgeOptions::default();
    let mut verb: Option<&str> = None;
    let mut index = 0usize;

    while index < args.len() {
        let argument = args[index].clone();
        match argument.as_str() {
            "--scan" => action = CliAction::Scan,
            "--version" | "-V" => action = CliAction::Version,
            "--help" | "-h" => action = CliAction::Help,
            "--json" => { /* rendering hint, the console always emits text */ }
            "--no-restore-point" => options.create_restore_point = false,
            "--no-uninstaller" => options.run_official_uninstaller = false,
            "--no-services" => options.remove_services = false,
            "--no-tasks" => options.remove_tasks = false,
            "--no-startup" => options.remove_startup = false,
            "--no-reboot-deferral" => options.defer_to_reboot = false,
            "--dry-run" => {
                // A dry run is an analysis that never escalates to a purge.
                verb = Some("analyze");
            }
            "--analyze" | "--purge" => {
                verb = Some(if argument == "--purge" { "purge" } else { "analyze" });
            }
            other if other.starts_with("--") => {
                eprintln!("unknown switch '{other}'");
                print_usage();
                return CliAction::Help;
            }
            other => {
                match verb {
                    Some("purge") => {
                        action = CliAction::Purge {
                            query: other.to_string(),
                            options: options.clone(),
                        };
                    }
                    _ => {
                        action = CliAction::Analyze(other.to_string());
                    }
                }
            }
        }
        index += 1;
    }

    action
}

/// Prints the usage banner.
pub fn print_usage() {
    println!(
        r#"DISINTEGRATOR v{}
Premium ultra-deep uninstaller for Windows 11

USAGE:
    disintegrator [OPTIONS]

TARGETING:
    --scan                     List every installed application
    --analyze <TARGET>         Deep inspection without destroying anything
    --purge <TARGET>           Execute the full purge pipeline

EXECUTION:
    --no-restore-point         Skip the system restore point creation
    --no-uninstaller           Do not invoke the vendor uninstaller
    --no-services              Keep Windows service registrations
    --no-tasks                 Keep scheduled tasks
    --no-startup               Keep autostart hooks
    --no-reboot-deferral       Fail instead of deferring locked files

OUTPUT:
    --json                     Emit machine readable JSON
    --version, -V              Print the version banner
    --help, -h                 Print this banner"#,
        env!("CARGO_PKG_VERSION")
    );
}

/// Executes a parsed action and returns the process exit code.
pub fn execute(action: CliAction) -> i32 {
    match action {
        CliAction::Help => {
            print_usage();
            0
        }
        CliAction::Version => {
            println!(
                "DISINTEGRATOR v{} ({})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH
            );
            0
        }
        CliAction::Scan => {
            let applications = apps::enumerate_applications();
            println!(
                "{:<44} {:<22} {:>12}  {}",
                "APPLICATION", "PUBLISHER", "SIZE", "SOURCE"
            );
            for application in &applications {
                println!(
                    "{:<44} {:<22} {:>12}  {}",
                    truncate(&application.display_name, 44),
                    truncate(application.publisher.as_deref().unwrap_or("—"), 22),
                    format_size(application.estimated_size_bytes),
                    application.source.label()
                );
            }
            println!("\n{} application(s) registered.", applications.len());
            0
        }
        CliAction::Analyze(query) => {
            let Some(application) = residue::resolve_target(&query) else {
                eprintln!("target '{query}' not found");
                return 2;
            };
            let report = residue::analyze(&application);
            println!("TARGET     {}", report.app.display_name);
            println!("THREAT     {} (score {})", report.threat.label(), report.score);
            println!("BYTES      {}", report.total_bytes);
            println!("ARTEFACTS  {}", report.items.len());
            println!();
            for item in &report.items {
                println!(
                    "  [{:<16}] {}{}",
                    item.kind.label(),
                    item.location,
                    if item.protected {
                        "  <PROTECTED>"
                    } else {
                        ""
                    }
                );
            }
            0
        }
        CliAction::Purge { query, options } => {
            let Some(application) = residue::resolve_target(&query) else {
                eprintln!("target '{query}' not found");
                return 2;
            };

            let mut sink = |event: PurgeEvent| {
                if event.stage == PurgeStage::Complete {
                    return;
                }
                println!("[{:>18}] {}", event.stage.label(), event.message);
            };

            let report = purge::purge(&application, &options, &mut sink);

            println!();
            println!("RESULT");
            println!("  bytes reclaimed     {}", report.bytes_reclaimed);
            println!("  files removed       {}", report.files_removed);
            println!("  files deferred      {}", report.files_deferred);
            println!("  registry keys       {}", report.registry_keys_removed);
            println!("  registry values     {}", report.registry_values_removed);
            println!("  services            {}", report.services_removed);
            println!("  tasks               {}", report.tasks_removed);
            println!("  startup hooks       {}", report.startup_removed);
            println!("  processes killed    {}", report.processes_terminated);
            println!("  reboot required     {}", report.reboot_required);
            println!("  duration            {} ms", report.duration_ms);

            if report.failures.is_empty() {
                0
            } else {
                1
            }
        }
        CliAction::Gui => 0,
    }
}

/// Shortens a value for table alignment.
fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    let mut result: String = value.chars().take(width.saturating_sub(1)).collect();
    result.push('…');
    result
}

/// Renders a byte count in a human readable form.
fn format_size(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "—".to_string();
    };
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}
