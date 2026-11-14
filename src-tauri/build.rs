fn main() {
    // The application manifest is embedded through tauri-build so that the
    // executable is stamped with `requireAdministrator` (UAC elevation) and
    // with the Common Controls v6 dependency required by native dialogs.
    //
    // The manifest is only applied to release builds: a test harness that
    // carries `requireAdministrator` cannot be executed by an unprivileged
    // runner, which would break `cargo test`.
    let release = std::env::var("PROFILE")
        .map(|profile| profile == "release")
        .unwrap_or(false);

    let windows_attributes = if release {
        tauri_build::WindowsAttributes::new()
            .app_manifest(include_str!("assets/windows-app.manifest"))
    } else {
        tauri_build::WindowsAttributes::new()
    };

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows_attributes))
        .expect("failed to run tauri-build");
}
