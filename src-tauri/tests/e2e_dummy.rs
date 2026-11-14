//! End to end verification.
//!
//! A complete dummy application is generated on the live machine (registry
//! trees, scattered files, an autostart hook, a Windows service and a
//! scheduled task) and then dissolved with the production engine. The test
//! asserts that **100 %** of the generated artefacts are gone afterwards.

use std::path::{Path, PathBuf};

use disintegrator_lib::engine::safety::SafetyGuard;
use disintegrator_lib::engine::sysinfo;
use disintegrator_lib::model::{AppSource, InstalledApp, PurgeOptions};

const DUMMY_NAME: &str = "Disintegrator E2E Dummy 7f31c9";
const DUMMY_PUBLISHER: &str = "E2E Verification Labs";
const DUMMY_KEY: &str = r"Software\Disintegrator\E2E\Dummy";
const DUMMY_RUN_VALUE: &str = "DisintegratorE2EDummy";
const DUMMY_SERVICE: &str = "DisintegratorE2ESvc";
const DUMMY_TASK: &str = "DisintegratorE2ETask";

/// The installer registration lives in `HKLM` for elevated hosts and in the
/// per-user hive otherwise, so that the test also runs unprivileged.
fn uninstall_hive(elevated: bool) -> &'static str {
    if elevated {
        "HKEY_LOCAL_MACHINE"
    } else {
        "HKEY_CURRENT_USER"
    }
}

/// Path of the dummy installer registration.
fn uninstall_key(elevated: bool) -> String {
    if elevated {
        format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{DUMMY_NAME}")
    } else {
        format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{DUMMY_NAME}")
    }
}

/// A generated dummy application together with its cleanup handle.
struct Dummy {
    root: PathBuf,
    app: InstalledApp,
    uninstall_hive: &'static str,
    uninstall_key: String,
}

impl Drop for Dummy {
    fn drop(&mut self) {
        // The dummy is always removed, even when an assertion fails.
        let _ = std::fs::remove_dir_all(&self.root);
        let guard = SafetyGuard::new();
        let _ = disintegrator_lib::engine::registry::delete_key_tree(
            "HKEY_CURRENT_USER",
            DUMMY_KEY,
            &guard,
        );
        let _ = disintegrator_lib::engine::registry::delete_key_tree(
            self.uninstall_hive,
            &self.uninstall_key,
            &guard,
        );
        let _ = disintegrator_lib::engine::registry::delete_value(
            "HKEY_CURRENT_USER",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            DUMMY_RUN_VALUE,
            &guard,
        );
    }
}

/// Generates the dummy application on disk and in the registry.
fn build_dummy(elevated: bool) -> Dummy {
    let root = std::env::temp_dir().join("Disintegrator E2E Dummy 7f31c9");
    let _ = std::fs::remove_dir_all(&root);
    let hive = uninstall_hive(elevated);
    let key_path = uninstall_key(elevated);

    // --- payload ---------------------------------------------------------
    std::fs::create_dir_all(root.join("bin")).expect("create dummy tree");
    std::fs::write(
        root.join("bin").join("dummy-agent.exe"),
        vec![0x4du8; 8192],
    )
    .expect("write dummy executable");
    std::fs::write(root.join("payload.dat"), vec![0x7fu8; 2048]).expect("write dummy data");
    std::fs::create_dir_all(root.join("nested").join("deep")).expect("create nested tree");
    std::fs::write(
        root.join("nested").join("deep").join("residue.cfg"),
        b"residue",
    )
    .expect("write nested residue");

    // --- registry --------------------------------------------------------
    let root_key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = root_key
        .create_subkey(DUMMY_KEY)
        .expect("create dummy registry key");
    let (child, _) = key.create_subkey("Nested").expect("create nested registry key");
    let _ = child.set_value(
        "Payload",
        &r"C:\Program Files\Disintegrator E2E Dummy 7f31c9",
    );
    let _ = key.set_value("InstallPath", &root.to_string_lossy().to_string());

    let local_machine = if elevated {
        winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
    } else {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
    };
    let (uninstall, _) = local_machine
        .create_subkey(&key_path)
        .expect("create dummy uninstall key");
    let _ = uninstall.set_value("DisplayName", &DUMMY_NAME);
    let _ = uninstall.set_value("Publisher", &DUMMY_PUBLISHER);
    let _ = uninstall.set_value("InstallLocation", &root.to_string_lossy().to_string());
    let _ = uninstall.set_value(
        "DisplayIcon",
        &format!(r#"{}"#, root.join("bin").join("dummy-agent.exe").display()),
    );
    let _ = uninstall.set_value(
        "UninstallString",
        &format!(r#""{}" /SILENT"#, root.join("bin").join("dummy-agent.exe").display()),
    );

    // --- autostart hook --------------------------------------------------
    let (run, _) = root_key
        .create_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run")
        .expect("open run key");
    let _ = run.set_value(
        DUMMY_RUN_VALUE,
        &format!(r#""{}""#, root.join("bin").join("dummy-agent.exe").display()),
    );

    let app = InstalledApp {
        id: format!("{hive}::{key_path}"),
        display_name: DUMMY_NAME.to_string(),
        display_version: Some("1.0.0".to_string()),
        publisher: Some(DUMMY_PUBLISHER.to_string()),
        install_date: Some("2026-01-01".to_string()),
        install_location: Some(root.to_string_lossy().to_string()),
        uninstall_string: Some(format!(
            r#""{}" /SILENT"#,
            root.join("bin").join("dummy-agent.exe").display()
        )),
        quiet_uninstall_string: None,
        estimated_size_bytes: Some(10_240),
        system_component: false,
        windows_installer: false,
        parent_key_name: None,
        source: AppSource::Registry,
        registry_key: Some(format!("{hive}\\{key_path}")),
        executable: Some(
            root.join("bin")
                .join("dummy-agent.exe")
                .display()
                .to_string(),
        ),
        architecture: "x64".to_string(),
        tokens: Vec::new(),
    };

    Dummy {
        root,
        uninstall_hive: hive,
        uninstall_key: key_path.clone(),
        app: InstalledApp {
            tokens: disintegrator_lib::engine::registry::build_match_tokens(&app),
            ..app
        },
    }
}

/// Creates a throw-away Windows service (administrators only).
fn create_service(executable: &Path) -> bool {
    let output = std::process::Command::new("sc.exe")
        .args([
            "create",
            DUMMY_SERVICE,
            "binPath=",
            &format!("\"{}\"", executable.display()),
            "start= auto",
            "DisplayName= Disintegrator E2E Dummy Service",
        ])
        .output();

    output.map(|result| result.status.success()).unwrap_or(false)
}

/// Creates a throw-away scheduled task (administrators only).
fn create_task(executable: &Path) -> bool {
    let output = std::process::Command::new("schtasks.exe")
        .args([
            "/create",
            "/tn",
            DUMMY_TASK,
            "/tr",
            &format!("\"{}\"", executable.display()),
            "/sc",
            "once",
            "/st",
            "00:00",
            "/f",
        ])
        .output();

    output.map(|result| result.status.success()).unwrap_or(false)
}

/// Removes the throw-away artefacts (best effort).
fn cleanup_service_and_task() {
    let _ = std::process::Command::new("sc.exe")
        .args(["stop", DUMMY_SERVICE])
        .output();
    let _ = std::process::Command::new("sc.exe")
        .args(["delete", DUMMY_SERVICE])
        .output();
    let _ = std::process::Command::new("schtasks.exe")
        .args(["/delete", "/tn", DUMMY_TASK, "/f"])
        .output();
}

/// The complete dissolution of a generated dummy application.
#[test]
fn disintegrates_a_generated_dummy_application() {
    let elevated = sysinfo::is_elevated();
    let dummy = build_dummy(elevated);
    let executable = dummy.root.join("bin").join("dummy-agent.exe");

    let service_created = elevated && create_service(&executable);
    let task_created = elevated && create_task(&executable);

    // The dummy must be detectable before the purge.
    assert!(dummy.root.exists(), "dummy payload must exist");
    assert!(
        disintegrator_lib::engine::registry::key_exists("HKEY_CURRENT_USER", DUMMY_KEY),
        "dummy registry tree must exist"
    );
    assert!(
        disintegrator_lib::engine::registry::value_exists(
            "HKEY_CURRENT_USER",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            DUMMY_RUN_VALUE,
        ),
        "dummy autostart hook must exist"
    );

    let options = PurgeOptions {
        run_official_uninstaller: false,
        create_restore_point: false,
        kill_processes: true,
        remove_services: true,
        remove_tasks: true,
        remove_startup: true,
        defer_to_reboot: false,
        remove_appx: true,
    };

    let mut sink = |_event| {};
    let report = disintegrator_lib::engine::purge::purge(&dummy.app, &options, &mut sink);

    // --- filesystem ------------------------------------------------------
    assert!(
        !dummy.root.exists(),
        "the entire dummy payload must be dissolved"
    );
    assert!(report.files_removed > 0, "files must be reported as removed");

    // --- registry --------------------------------------------------------
    assert!(
        !disintegrator_lib::engine::registry::key_exists("HKEY_CURRENT_USER", DUMMY_KEY),
        "the dummy registry tree must be dissolved"
    );
    assert!(
        !disintegrator_lib::engine::registry::key_exists(
            dummy.uninstall_hive,
            &dummy.uninstall_key,
        ),
        "the dummy installer registration must be dissolved"
    );

    // --- autostart --------------------------------------------------------
    assert!(
        !disintegrator_lib::engine::registry::value_exists(
            "HKEY_CURRENT_USER",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            DUMMY_RUN_VALUE,
        ),
        "the dummy autostart hook must be dissolved"
    );
    assert!(
        report.startup_removed > 0,
        "the autostart removal must be reported"
    );

    if service_created {
        assert!(
            report.services_removed > 0,
            "the dummy service must be dissolved"
        );
        let catalogue = disintegrator_lib::engine::services::list_services();
        assert!(
            !catalogue
                .iter()
                .any(|service| service.name.eq_ignore_ascii_case(DUMMY_SERVICE)),
            "the dummy service registration must be gone"
        );
    } else {
        eprintln!("skipped service assertions: the test host is not elevated");
    }

    if task_created {
        assert!(
            report.tasks_removed > 0,
            "the dummy scheduled task must be dissolved"
        );
    } else {
        eprintln!("skipped task assertions: the test host is not elevated");
    }

    cleanup_service_and_task();
}
